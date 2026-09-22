use crate::types::DnsStatus;
use std::net::SocketAddr;
use std::process::Command;
use std::sync::Mutex;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::watch;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Global shutdown sender — signals proxy tasks to stop.
/// `watch` (not mpsc) so every listener can hold its own receiver and
/// `send` is callable from a synchronous context. mpsc needed an await,
/// so the sync `stop_dns_proxy` could only drop the sender and the TCP
/// accept loop — which never watched it at all — kept port 53 bound.
/// Wrapped in Mutex<Option<>> so it can be replaced on each start.
static SHUTDOWN: Mutex<Option<watch::Sender<bool>>> = Mutex::new(None);

// ─── System DNS helpers (netsh) ───────────────────────────────────────

/// Spawn a console child without flashing a CMD window.
/// This is a GUI-subsystem app, so Windows gives every netsh/netstat/tasklist
/// its own console — and `get_net_speed` runs netstat once a second, which
/// put a window in front of the desktop fast enough to make the machine
/// unusable. CREATE_NO_WINDOW = 0x08000000.
pub(crate) fn cmd(program: &str) -> Command {
    let mut c = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

/// Every connected non-loopback interface name, most likely first.
/// More than one because the state column is localized and the guessed
/// name may simply not exist — trying them in turn beats guessing once.
#[cfg(target_os = "windows")]
fn interface_candidates() -> Vec<String> {
    let mut found = Vec::new();
    if let Ok(output) = cmd("netsh")
        .args(["interface", "ip", "show", "interfaces"])
        .output()
    {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            // Idx Met MTU State Name — Idx must be numeric or this is the
            // other `show interface` layout, where "Connected" is column 0
            // and joining after it would swallow the Type column too.
            if parts.len() < 4 || parts[0].parse::<u32>().is_err() {
                continue;
            }
            if line.to_lowercase().contains("loopback") {
                continue;
            }
            if let Some(idx) = parts.iter().position(|p| p.eq_ignore_ascii_case("connected")) {
                if idx + 1 < parts.len() {
                    found.push(parts[idx + 1..].join(" "));
                }
            }
        }
    }
    for guess in ["Ethernet", "Wi-Fi", "Local Area Connection"] {
        if !found.iter().any(|f| f == guess) {
            found.push(guess.to_string());
        }
    }
    found
}

#[cfg(not(target_os = "windows"))]
fn interface_candidates() -> Vec<String> {
    vec!["eth0".to_string()]
}

#[cfg(target_os = "windows")]
fn get_active_interface() -> Result<String, String> {
    Ok(interface_candidates()
        .into_iter()
        .next()
        .unwrap_or_else(|| "Ethernet".to_string()))
}

#[cfg(not(target_os = "windows"))]
fn get_active_interface() -> Result<String, String> {
    Ok("eth0".into())
}

/// netsh writes failures to stdout, not stderr — the old message rendered
/// as a bare "netsh set dns failed:" and hid the actual reason.
fn netsh_out(out: &std::process::Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !err.is_empty() {
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(&err);
    }
    if s.is_empty() {
        format!("exit {:?}", out.status.code())
    } else {
        s
    }
}

/// Run `netsh ... set dns <iface> ...` against every candidate interface
/// until one accepts.
#[cfg(target_os = "windows")]
fn netsh_set_dns(args: &[&str], what: &str) -> Result<(), String> {
    let mut errs = Vec::new();
    for iface in interface_candidates() {
        let mut full = vec!["interface", "ip", "set", "dns", iface.as_str()];
        full.extend_from_slice(args);
        let out = match cmd("netsh").args(&full).output() {
            Ok(o) => o,
            Err(e) => return Err(format!("failed to run netsh: {e}")),
        };
        if out.status.success() {
            return Ok(());
        }
        errs.push(format!("{iface}: {}", netsh_out(&out)));
    }
    Err(format!("netsh {what} failed -> {}", errs.join(" | ")))
}

/// Set system DNS to a specific IP (used to point at our local proxy).
#[cfg(target_os = "windows")]
fn set_system_dns(dns_ip: &str) -> Result<(), String> {
    netsh_set_dns(&["static", dns_ip], "set dns")
}

#[cfg(not(target_os = "windows"))]
fn set_system_dns(_dns_ip: &str) -> Result<(), String> {
    Ok(())
}

/// Restore system DNS to DHCP (automatic).
#[cfg(target_os = "windows")]
pub fn restore_system_dns() -> Result<(), String> {
    netsh_set_dns(&["dhcp"], "restore dns")
}

#[cfg(not(target_os = "windows"))]
pub fn restore_system_dns() -> Result<(), String> {
    Ok(())
}

// ─── DNS Proxy ────────────────────────────────────────────────────────

/// Forward a single UDP DNS packet to the relay and return the response.
async fn forward_udp(packet: &[u8], relay: SocketAddr) -> Result<Vec<u8>, String> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("UDP bind failed: {e}"))?;

    socket
        .send_to(packet, relay)
        .await
        .map_err(|e| format!("UDP send failed: {e}"))?;

    let mut buf = vec![0u8; 4096];
    let n = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        socket.recv_from(&mut buf),
    )
    .await
    .map_err(|_| "UDP relay timeout".to_string())?
    .map_err(|e| format!("UDP recv failed: {e}"))?;

    buf.truncate(n.0);
    Ok(buf)
}

/// Handle one TCP DNS connection: read length-prefixed message, forward, reply.
async fn handle_tcp(mut client: TcpStream, relay: SocketAddr) {
    let mut len_buf = [0u8; 2];
    if client.read_exact(&mut len_buf).await.is_err() {
        return;
    }
    let msg_len = u16::from_be_bytes(len_buf) as usize;

    let mut msg = vec![0u8; msg_len];
    if client.read_exact(&mut msg).await.is_err() {
        return;
    }

    let response = match forward_tcp_raw(&msg, relay).await {
        Ok(r) => r,
        Err(_) => return,
    };

    let resp_len = (response.len() as u16).to_be_bytes();
    let _ = client.write_all(&resp_len).await;
    let _ = client.write_all(&response).await;
}

/// Forward raw DNS message via TCP to relay.
async fn forward_tcp_raw(packet: &[u8], relay: SocketAddr) -> Result<Vec<u8>, String> {
    let mut stream = TcpStream::connect(relay)
        .await
        .map_err(|e| format!("TCP connect failed: {e}"))?;

    stream.set_nodelay(true).ok();

    let len = (packet.len() as u16).to_be_bytes();
    stream.write_all(&len).await
        .map_err(|e| format!("TCP write len failed: {e}"))?;
    stream.write_all(packet).await
        .map_err(|e| format!("TCP write msg failed: {e}"))?;

    let mut len_buf = [0u8; 2];
    stream.read_exact(&mut len_buf).await
        .map_err(|e| format!("TCP read len failed: {e}"))?;
    let resp_len = u16::from_be_bytes(len_buf) as usize;

    let mut resp = vec![0u8; resp_len];
    stream.read_exact(&mut resp).await
        .map_err(|e| format!("TCP read msg failed: {e}"))?;

    Ok(resp)
}

/// Name the process holding port 53 — the old message blamed the DNS Client
/// service, which almost never actually owns the port.
fn port_holder(proto: &str) -> Option<String> {
    let out = cmd("netstat")
        .args(["-ano", "-p", &proto.to_ascii_lowercase()])
        .output()
        .ok()?;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 4 || !f[1].ends_with(":53") {
            continue;
        }
        let pid = f.last()?.parse::<u32>().ok()?;
        let t = cmd("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .ok()?;
        let name = String::from_utf8_lossy(&t.stdout)
            .lines()
            .next()?
            .split('"')
            .nth(1)?
            .to_string();
        if name.is_empty() || name.contains("No tasks") {
            return None;
        }
        return Some(format!("{name} (PID {pid})"));
    }
    None
}

fn bind_error(proto: &str, e: std::io::Error) -> String {
    if e.kind() != std::io::ErrorKind::AddrInUse {
        return format!("Failed to bind {proto} port 53: {e}");
    }
    let holder = port_holder(proto)
        .or_else(|| port_holder(if proto == "TCP" { "UDP" } else { "TCP" }));
    match holder {
        Some(who) => format!(
            "پورت 53 توسط «{who}» اشغال است. آن برنامه را ببندید یا سرویس‌اش را متوقف کنید، بعد دوباره اتصال بزنید."
        ),
        None => "پورت 53 اشغال است. نرم‌افزار DNS دیگری (Docker، AdGuard، Acrylic، ICS) را ببندید و دوباره امتحان کنید."
            .to_string(),
    }
}

/// Start the DNS proxy as a background tokio task.
/// Binds UDP+TCP on 127.0.0.1:53, forwards to relay_ip:53.
/// Changes system DNS to 127.0.0.1.
async fn start_proxy(relay_ip: String) -> Result<(), String> {
    // Stop any existing proxy first
    stop_proxy_inner().await;

    let relay_addr: SocketAddr = format!("{}:53", relay_ip)
        .parse()
        .map_err(|_| format!("invalid relay IP: {}", relay_ip))?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Loopback only: system DNS points at 127.0.0.1, so we never need
    // 0.0.0.0 — which both collides with ICS/Docker binds on other local
    // addresses and exposes an open resolver to the whole LAN.
    let udp = UdpSocket::bind("127.0.0.1:53")
        .await
        .map_err(|e| bind_error("UDP", e))?;
    let udp = std::sync::Arc::new(udp);

    let tcp = TcpListener::bind("127.0.0.1:53")
        .await
        .map_err(|e| bind_error("TCP", e))?;

    // Change system DNS to localhost
    set_system_dns("127.0.0.1")?;

    eprintln!("[PeDitXCDN] DNS proxy started: 127.0.0.1:53 -> {}:53", relay_ip);

    // Store shutdown sender
    *SHUTDOWN.lock().unwrap() = Some(shutdown_tx);

    // Spawn TCP accept loop — must watch shutdown too, otherwise it keeps
    // port 53 bound after disconnect and the next connect fails.
    let tcp_relay = relay_addr;
    let mut tcp_stop = shutdown_rx.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                accepted = tcp.accept() => {
                    match accepted {
                        Ok((stream, _)) => {
                            let relay = tcp_relay;
                            tokio::spawn(async move { handle_tcp(stream, relay).await; });
                        }
                        Err(e) => {
                            eprintln!("[PeDitXCDN] TCP accept error: {e}");
                            break;
                        }
                    }
                }
                _ = tcp_stop.changed() => break,
            }
        }
    });

    // Spawn UDP forwarding loop (non-blocking — returns immediately)
    let udp_clone = udp.clone();
    let mut udp_stop = shutdown_rx;
    tokio::spawn(async move {
        let mut recv_buf = [0u8; 4096];
        loop {
            tokio::select! {
                result = udp_clone.recv_from(&mut recv_buf) => {
                    if let Ok((n, peer)) = result {
                        let packet = recv_buf[..n].to_vec();
                        let sock = udp_clone.clone();
                        tokio::spawn(async move {
                            match forward_udp(&packet, relay_addr).await {
                                Ok(response) => {
                                    let _ = sock.send_to(&response, peer).await;
                                }
                                Err(e) => {
                                    eprintln!("[PeDitXCDN] UDP forward error: {e}");
                                }
                            }
                        });
                    }
                }
                _ = udp_stop.changed() => {
                    eprintln!("[PeDitXCDN] DNS proxy stopped");
                    break;
                }
            }
        }
    });

    Ok(())
}

/// Stop the proxy and restore system DNS.
async fn stop_proxy_inner() {
    // Send shutdown signal (sync — watch::Sender::send needs no await)
    let tx = SHUTDOWN.lock().unwrap().take();
    if let Some(tx) = tx {
        let _ = tx.send(true);
        // Wait briefly for tasks to exit
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }

    // Restore system DNS
    let _ = restore_system_dns();
    eprintln!("[PeDitXCDN] System DNS restored to DHCP");
}

// ─── Public API (called from Tauri commands) ──────────────────────────

/// Start the DNS proxy + change system DNS.
pub fn start_dns_proxy(relay_ip: &str) -> Result<(), String> {
    let rt = tauri::async_runtime::handle();
    rt.block_on(start_proxy(relay_ip.to_string()))
}

/// Stop the DNS proxy + restore system DNS (non-blocking, safe from any thread).
pub fn stop_dns_proxy() {
    // Send the shutdown signal instead of just dropping the sender —
    // dropping stopped the UDP loop but left the TCP listener bound.
    if let Some(tx) = SHUTDOWN.lock().unwrap().take() {
        let _ = tx.send(true);
        // Give the accept/recv loops time to drop their sockets. Without
        // this an immediate reconnect raced them and bind :53 failed with
        // "port 53 is in use" — our own listener from a moment ago.
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    // Restore DNS directly (sync, no tokio needed)
    let _ = restore_system_dns();
    eprintln!("[PeDitXCDN] DNS proxy stopped, system DNS restored to DHCP");
}

/// Stop the DNS proxy + restore system DNS (async version for non-tokio threads).
pub async fn stop_dns_proxy_async() {
    stop_proxy_inner().await;
}

/// Get current DNS configuration status.
pub fn get_dns_status() -> Result<DnsStatus, String> {
    #[cfg(target_os = "windows")]
    {
        let iface = get_active_interface()?;
        let output = cmd("netsh")
            .args(["interface", "ip", "show", "dns", &iface])
            .output()
            .map_err(|e| format!("failed to run netsh: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let configured = stdout.to_lowercase().contains("static");

        let current_dns = stdout
            .lines()
            .find(|line| {
                let l = line.trim().to_lowercase();
                l.contains('.') && !l.contains("dns servers") && !l.contains("configuration")
            })
            .map(|line| line.trim().to_string());

        let is_relay_dns = current_dns.as_deref() == Some("127.0.0.1");

        Ok(DnsStatus {
            configured,
            current_dns,
            interface: iface,
            is_relay_dns,
        })
    }

    #[cfg(not(target_os = "windows"))]
    Ok(DnsStatus {
        configured: false,
        current_dns: None,
        interface: "N/A".into(),
        is_relay_dns: false,
    })
}

/// Previous octet counters, for turning cumulative totals into a rate.
static NET_LAST: Mutex<Option<(std::time::Instant, u64, u64)>> = Mutex::new(None);

/// Pull `Received` / `Sent` byte totals out of `netstat -e` output.
/// ponytail: byte-offset match instead of `str[..5]` — a localized header
/// would otherwise panic on a char boundary. Upgrade path: GetIfEntry2.
fn parse_netstat_e(text: &str) -> Option<(u64, u64)> {
    for line in text.lines() {
        let t = line.trim_start();
        if t.len() < 5 || !t.as_bytes()[..5].eq_ignore_ascii_case(b"bytes") {
            continue;
        }
        let nums: Vec<u64> = t.split_whitespace().filter_map(|w| w.parse().ok()).collect();
        // Columns are Received, then Sent.
        if nums.len() >= 2 {
            return Some((nums[0], nums[1]));
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn read_octets() -> Result<(u64, u64), String> {
    let out = cmd("netstat")
        .args(["-e"])
        .output()
        .map_err(|e| format!("netstat -e failed: {e}"))?;
    parse_netstat_e(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| "netstat -e: no Bytes line".to_string())
}

#[cfg(not(target_os = "windows"))]
fn read_octets() -> Result<(u64, u64), String> {
    Ok((0, 0))
}

/// Current system-wide upload/download rate in bytes per second, (recv, sent).
/// First call has nothing to diff against and returns zeros.
pub fn get_net_speed() -> Result<(u64, u64), String> {
    let (rx, tx) = read_octets()?;
    let mut last = NET_LAST.lock().unwrap();
    let now = std::time::Instant::now();
    let rate = match *last {
        Some((t0, r0, s0)) => {
            let dt = now.duration_since(t0).as_secs_f64();
            if dt < 0.2 {
                return Ok((0, 0));
            }
            (
                (rx.saturating_sub(r0) as f64 / dt) as u64,
                (tx.saturating_sub(s0) as f64 / dt) as u64,
            )
        }
        None => (0, 0),
    };
    *last = Some((now, rx, tx));
    Ok(rate)
}

/// Resolve the panel URL host to the relay's IPv4 address — the DNS forward
/// target. Panel and relay share a host, so this is the only address the
/// proxy should ever point at. `info.ip` / `info.seen_ip` are the *user's*
/// address (ACL key), never the relay's.
pub fn resolve_relay_ip(panel_url: &str) -> Result<String, String> {
    use std::net::ToSocketAddrs;
    let host = reqwest::Url::parse(panel_url)
        .map_err(|e| format!("invalid panel URL: {e}"))?
        .host_str()
        .ok_or_else(|| "panel URL has no host".to_string())?
        .to_string();
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        return Ok(host);
    }
    (host.as_str(), 443u16)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve panel host {host}: {e}"))?
        .find(|addr| addr.is_ipv4())
        .map(|addr| addr.ip().to_string())
        .ok_or_else(|| format!("no IPv4 address for {host}"))
}

/// Resolve `domain` through the local proxy (127.0.0.1:53).
/// This is the visible proof that a connection is real: a hijacked domain
/// answers with the relay IP, a bypassed one with a public address, and no
/// answer at all means the proxy or the relay is down.
/// Pull answer addresses out of `nslookup` output for both platforms.
/// The server line is 127.0.0.1 (skipped as loopback), answers may come as
/// `Address: 1.2.3.4`, a bare `1.2.3.4` under `Addresses:`, or the `#53`
/// authority form — so trim everything that is not part of an address.
fn nslookup_addrs(stdout: &str, stderr: &str) -> Vec<String> {
    let mut ips: Vec<String> = Vec::new();
    for line in stdout.lines().chain(stderr.lines()) {
        for tok in line.split_whitespace() {
            let tok = tok.trim_matches(|c: char| !c.is_ascii_digit() && c != ':' && c != '.');
            if let Ok(ip) = tok.parse::<std::net::IpAddr>() {
                if ip.is_loopback() {
                    continue;
                }
                let s = ip.to_string();
                if !ips.contains(&s) {
                    ips.push(s);
                }
            }
        }
    }
    ips
}

pub fn resolve_local(domain: &str) -> Result<Vec<String>, String> {
    let out = cmd("nslookup")
        .args([domain, "127.0.0.1"])
        .output()
        .map_err(|e| format!("nslookup failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let ips = nslookup_addrs(&stdout, &stderr);

    if ips.is_empty() {
        Err(format!(
            "پاسخی از پروکسی محلی نیامد: {}",
            netsh_out(&out)
        ))
    } else {
        Ok(ips)
    }
}

/// Ping the relay IP to check connectivity.
pub fn check_relay_connection(relay_ip: &str) -> Result<bool, String> {
    let output = if cfg!(target_os = "windows") {
        cmd("ping")
            .args(["-n", "1", "-w", "1000", relay_ip])
            .output()
    } else {
        cmd("ping")
            .args(["-c", "1", "-W", "1", relay_ip])
            .output()
    };

    let result = output.map_err(|e| format!("ping failed: {e}"))?;
    Ok(result.status.success())
}
