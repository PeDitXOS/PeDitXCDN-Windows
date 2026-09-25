use crate::types::{DnsStatus, EmergencyStop, LocalResolve, ProxyStatus};
use crate::wireproxy;
use std::net::SocketAddr;
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
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

// ─── Service state ────────────────────────────────────────────────────

/// What is actually running right now. The webview keeps its own
/// `connected` flag, which happily kept saying «متصل» after the loops were
/// gone — control has to ask the backend, and it has to be cheap enough to
/// poll every second (this is a Mutex read, no netsh).
static PROXY_STATE: Mutex<Option<ProxyState>> = Mutex::new(None);

/// Last relay we started with, so the tray's «اتصال» can restart the proxy
/// without a round-trip through the webview.
static LAST_RELAY: Mutex<Option<String>> = Mutex::new(None);

struct ProxyState {
    relay: String,
    started: std::time::Instant,
    v6: bool,
    fragment: bool,
}

/// Bumped by every Disconnect / emergency cut. `start_proxy` snapshots it on
/// entry and re-checks at each phase boundary, so a connect the user started
/// by mistake unwinds — and puts DNS back — instead of finishing behind a UI
/// that already says «قطع». A generation, not a bool: the next press takes a
/// fresh snapshot, so cancelling one start can never cancel the one after it.
static CANCEL_GEN: AtomicU64 = AtomicU64::new(0);

fn bump_cancel() {
    CANCEL_GEN.fetch_add(1, Ordering::SeqCst);
}

/// Err only when this particular press was abandoned. The webview drops the
/// message (its own generation guard has already moved on), so it only has to
/// be recognizable in `debug.log`.
fn cancelled(gen: u64) -> Result<(), String> {
    if CANCEL_GEN.load(Ordering::SeqCst) == gen {
        Ok(())
    } else {
        Err("اتصال توسط کاربر لغو شد".into())
    }
}

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

/// Run a child and file a `SLOW` line when it took over 400 ms. Connect is a
/// chain of these (`show interfaces` + one `set dns` per interface, twice),
/// so a single netsh that stalls is invisible in the log — and it is the only
/// thing that can turn a press into minutes. eprintln never reaches the log
/// (GUI child, CREATE_NO_WINDOW), hence `log_to_file`.
pub(crate) fn run(program: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    let t = std::time::Instant::now();
    let out = cmd(program).args(args).output();
    let ms = t.elapsed().as_millis();
    if ms > 400 {
        let _ = crate::api::log_to_file(&format!("SLOW {ms}ms {program} {}", args.join(" ")));
    }
    out
}

/// Every connected non-loopback interface name, most likely first.
/// More than one because the state column is localized and the guessed
/// name may simply not exist — trying them in turn beats guessing once.
#[cfg(target_os = "windows")]
fn interface_candidates() -> Vec<String> {
    let mut found = Vec::new();
    if let Ok(output) = run("netsh", &["interface", "ip", "show", "interfaces"]) {
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
    // Guess only when the parse found nothing: the guesses exist for a
    // localized/unfamiliar layout, and every miss costs a netsh call (~0.5s)
    // that would otherwise run on each connect, on all of them plus IPv6.
    if found.is_empty() {
        for guess in ["Ethernet", "Wi-Fi", "Local Area Connection"] {
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

/// Run `netsh ... set dns <iface> ...` against **every** candidate interface.
/// Stopping at the first success left the rest on the ISP's resolver, and
/// Windows asks all of them at once (Smart Multi-Homed Name Resolution) —
/// the ISP answers in ~10 ms, our relay path in ~240 ms, so the ISP's
/// blocked answer always won the race. At least one must accept.
///
/// `cancel` is the caller's `CANCEL_GEN` snapshot: this loop is the slow half
/// of a connect, so it must be able to give up between interfaces instead of
/// running to the end after the user pressed Cancel. The restore path passes
/// `None` — DNS is going back to DHCP and must not be abandoned halfway.
#[cfg(target_os = "windows")]
fn netsh_set_dns(args: &[&str], what: &str, cancel: Option<u64>) -> Result<(), String> {
    let mut errs = Vec::new();
    let mut ok = 0;
    for iface in interface_candidates() {
        if let Some(g) = cancel {
            cancelled(g)?;
        }
        let mut full = vec!["interface", "ip", "set", "dns", iface.as_str()];
        full.extend_from_slice(args);
        let out = match run("netsh", &full) {
            Ok(o) => o,
            Err(e) => return Err(format!("failed to run netsh: {e}")),
        };
        if out.status.success() {
            ok += 1;
        } else {
            errs.push(format!("{iface}: {}", netsh_out(&out)));
        }
    }
    if ok > 0 {
        Ok(())
    } else {
        Err(format!("netsh {what} failed -> {}", errs.join(" | ")))
    }
}

/// Windows' DNS Client keeps answering from cache for minutes. Without a
/// flush the pre-connect (ISP) result keeps being served after we repoint DNS.
#[cfg(target_os = "windows")]
fn flush_dns_cache() {
    let _ = run("ipconfig", &["/flushdns"]);
}

/// The IPv6 resolvers on the same interface are a second path straight
/// around the proxy (SMHNR races them too). Point them at our own [::1]:53.
/// Non-fatal: a failed set just leaves IPv6 unproxied, which the AAAA probe
/// then shows as «مستقیم».
#[cfg(target_os = "windows")]
fn set_ipv6_dns(addr: &str, gen: u64) -> Result<(), String> {
    for iface in interface_candidates() {
        cancelled(gen)?;
        let args = ["interface", "ipv6", "set", "dns", iface.as_str(), "static", addr, "primary"];
        match run("netsh", &args) {
            Ok(o) if o.status.success() => {}
            Ok(o) => eprintln!("[PeDitXCDN] ipv6 set dns {iface}: {}", netsh_out(&o)),
            Err(e) => eprintln!("[PeDitXCDN] ipv6 set dns {iface}: {e}"),
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn restore_ipv6_dns() {
    for iface in interface_candidates() {
        // Two spellings: older builds take the source positionally, newer
        // ones want source=. Best effort — a miss leaves ::1 pointing at a
        // proxy that is gone, which the next start fixes anyway.
        let attempts: [Vec<&str>; 2] = [
            vec!["interface", "ipv6", "set", "dns", iface.as_str(), "dhcp"],
            vec!["interface", "ipv6", "set", "dns", iface.as_str(), "source=dhcp"],
        ];
        let mut done = false;
        for args in attempts {
            if let Ok(o) = run("netsh", &args) {
                if o.status.success() {
                    done = true;
                    break;
                }
            }
        }
        if !done {
            eprintln!("[PeDitXCDN] ipv6 restore dns {iface} failed");
        }
    }
}

/// Set system DNS to a specific IP (used to point at our local proxy).
/// `proxy_v6` — the proxy is also listening on [::1]:53, so it is safe to
/// send the IPv6 resolvers there. When it is not, leaving the ISP's IPv6 DNS
/// alone beats pointing it at a dead address (total breakage instead of a leak).
#[cfg(target_os = "windows")]
fn set_system_dns(dns_ip: &str, proxy_v6: bool, gen: u64) -> Result<(), String> {
    netsh_set_dns(&["static", dns_ip], "set dns", Some(gen))?;
    if proxy_v6 {
        set_ipv6_dns("::1", gen)?;
    } else {
        // Not "leave IPv6 alone" any more: the connect path no longer restores
        // DHCP before repointing, so a stale ::1 from a killed session would
        // otherwise survive this start and black-hole resolution. Going back
        // to dhcp removes the dead address without pointing at one.
        restore_ipv6_dns();
        eprintln!("[PeDitXCDN] [::1]:53 not bound — IPv6 DNS returned to dhcp (no leak, no dead ::1)");
    }
    // Both stacks are repointed now: an abandoned press unwinds here, before
    // the cache flush, and the caller's error path puts DHCP back.
    cancelled(gen)?;
    flush_dns_cache();
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn set_system_dns(_dns_ip: &str, _proxy_v6: bool, _gen: u64) -> Result<(), String> {
    Ok(())
}

/// Restore system DNS to DHCP (automatic).
#[cfg(target_os = "windows")]
pub fn restore_system_dns() -> Result<(), String> {
    let r = netsh_set_dns(&["dhcp"], "restore dns", None);
    restore_ipv6_dns();
    flush_dns_cache();
    r
}

#[cfg(not(target_os = "windows"))]
pub fn restore_system_dns() -> Result<(), String> {
    Ok(())
}

const QTYPE_A: u16 = 1;
const QTYPE_AAAA: u16 = 28;

/// Skip a DNS name at `i` (compression pointers included) → offset after it.
fn skip_name(msg: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let len = *msg.get(i)? as usize;
        if len == 0 {
            return Some(i + 1);
        }
        if len & 0xC0 == 0xC0 {
            return Some(i + 2);
        }
        i += len + 1;
    }
}

/// Offset just past the single question: name + qtype + qclass.
fn question_end(msg: &[u8]) -> Option<usize> {
    let end = skip_name(msg, 12)?;
    if end + 4 > msg.len() {
        None
    } else {
        Some(end + 4)
    }
}

fn is_aaaa_query(msg: &[u8]) -> bool {
    if msg.len() < 12 || u16::from_be_bytes([msg[4], msg[5]]) != 1 {
        return false;
    }
    match question_end(msg) {
        Some(end) => u16::from_be_bytes([msg[end - 4], msg[end - 3]]) == QTYPE_AAAA,
        None => false,
    }
}

/// Local NOERROR/0-answer for an AAAA query, instead of forwarding it.
/// dnsmasq's `address=/domain/IP` overrides the **A** record only — the
/// relay hands the public AAAA straight back, Windows picks IPv6, and the
/// browser leaves the tunnel for the blocked path. "No AAAA here" forces
/// IPv4, which is the record the hijack rewrites.
/// ponytail: disables IPv6 for every name, IPv6-only ones included.
/// Upgrade path: forward the AAAA and rewrite its RDATA to a relay IPv6.
fn nodata_aaaa(msg: &[u8]) -> Option<Vec<u8>> {
    if !is_aaaa_query(msg) {
        return None;
    }
    let end = question_end(msg)?;
    let mut r = Vec::with_capacity(end);
    r.extend_from_slice(&msg[0..2]);   // transaction id
    r.push(0x80 | (msg[2] & 0x01));    // QR=1, keep the query's RD
    r.push(0x80);                      // RA=1, RCODE=NOERROR
    r.extend_from_slice(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    r.extend_from_slice(&msg[12..end]); // echo the question
    Some(r)
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

/// How sick the relay's UDP path looks. Non-zero means UDP is losing (or
/// failing) — a middlebox dropping UDP/53 does exactly this — so the next
/// query starts the TCP attempt in flight instead of waiting out the stagger.
/// A UDP win resets it: that is the healthy path, no extra handshake.
static UDP_SICK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// UDP first, TCP as soon as UDP looks dead — first answer wins.
/// A healthy path never pays for the TCP handshake; a path that swallows
/// UDP/53 still resolves (the probe only waits 2s, so TCP has to be in
/// flight early, not as a retry after the 3s UDP timeout).
async fn forward_best(packet: &[u8], relay: SocketAddr) -> Result<Vec<u8>, String> {
    use std::sync::atomic::Ordering;
    let staggered = UDP_SICK.load(Ordering::Relaxed) == 0;
    let mut udp = Box::pin(forward_udp(packet, relay));
    let mut tcp = Box::pin(async move {
        if staggered {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
        forward_tcp_raw(packet, relay).await
    });

    let mut first_err: Option<String> = None;
    let mut udp_done = false;
    let mut tcp_done = false;
    while !udp_done || !tcp_done {
        tokio::select! {
            r = &mut udp, if !udp_done => {
                udp_done = true;
                match r {
                    Ok(v) => {
                        UDP_SICK.store(0, Ordering::Relaxed);
                        return Ok(v);
                    }
                    Err(e) => {
                        UDP_SICK.fetch_add(1, Ordering::Relaxed);
                        if first_err.is_none() {
                            first_err = Some(e);
                        }
                    }
                }
            }
            r = &mut tcp, if !tcp_done => {
                tcp_done = true;
                match r {
                    Ok(v) => {
                        // TCP beat UDP — the UDP leg is what is broken, and
                        // counting it only on a UDP *error* never fires: a
                        // silently dropped UDP query is still in flight when
                        // TCP answers, so the path would stay "healthy" and
                        // pay the stagger forever.
                        UDP_SICK.fetch_add(1, Ordering::Relaxed);
                        return Ok(v);
                    }
                    Err(e) => {
                        if first_err.is_none() {
                            first_err = Some(e);
                        }
                    }
                }
            }
        }
    }
    Err(first_err.unwrap_or_else(|| "no upstream answer".to_string()))
}

/// Handle one TCP DNS connection: read length-prefixed message, forward, reply.
/// `rewrite` (set only while the local wire listeners are up) swaps the
/// relay's own A answers for 127.0.0.1 so the app connects to our local
/// listeners instead of straight to the relay.
async fn handle_tcp(mut client: TcpStream, relay: SocketAddr, rewrite: Option<[u8; 4]>) {
    let mut len_buf = [0u8; 2];
    if client.read_exact(&mut len_buf).await.is_err() {
        return;
    }
    let msg_len = u16::from_be_bytes(len_buf) as usize;

    let mut msg = vec![0u8; msg_len];
    if client.read_exact(&mut msg).await.is_err() {
        return;
    }

    let mut response = match nodata_aaaa(&msg) {
        Some(r) => r,
        None => match forward_tcp_raw(&msg, relay).await {
            Ok(r) => r,
            Err(_) => return,
        },
    };
    if let Some(from) = rewrite {
        wireproxy::rewrite_relay_a(&mut response, from, [127, 0, 0, 1]);
    }

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

/// TCP accept loop for one listener. Watches shutdown — otherwise it keeps
/// port 53 bound after disconnect and the next connect fails.
fn spawn_tcp_loop(
    lst: TcpListener,
    relay: SocketAddr,
    rewrite: Option<[u8; 4]>,
    mut stop: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                accepted = lst.accept() => {
                    match accepted {
                        Ok((stream, _)) => {
                            tokio::spawn(async move {
                                handle_tcp(stream, relay, rewrite).await
                            });
                        }
                        Err(e) => {
                            eprintln!("[PeDitXCDN] TCP accept error: {e}");
                            break;
                        }
                    }
                }
                _ = stop.changed() => break,
            }
        }
    });
}

/// UDP forward loop for one socket; AAAA is answered locally (nodata_aaaa).
fn spawn_udp_loop(
    sock: std::sync::Arc<UdpSocket>,
    relay: SocketAddr,
    rewrite: Option<[u8; 4]>,
    mut stop: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let mut recv_buf = [0u8; 4096];
        loop {
            tokio::select! {
                result = sock.recv_from(&mut recv_buf) => {
                    if let Ok((n, peer)) = result {
                        let packet = recv_buf[..n].to_vec();
                        let s = sock.clone();
                        tokio::spawn(async move {
                            if let Some(resp) = nodata_aaaa(&packet) {
                                let _ = s.send_to(&resp, peer).await;
                                return;
                            }
                            match forward_best(&packet, relay).await {
                                Ok(mut response) => {
                                    if let Some(from) = rewrite {
                                        wireproxy::rewrite_relay_a(
                                            &mut response,
                                            from,
                                            [127, 0, 0, 1],
                                        );
                                    }
                                    let _ = s.send_to(&response, peer).await;
                                }
                                Err(e) => {
                                    // eprintln vanishes for a GUI child — this
                                    // line is what tells us *which* hop died.
                                    crate::api::log_to_file(&format!(
                                        "FORWARD FAIL {relay} ({peer}): {e}"
                                    ));
                                    eprintln!("[PeDitXCDN] forward error: {e}");
                                }
                            }
                        });
                    }
                }
                _ = stop.changed() => {
                    eprintln!("[PeDitXCDN] DNS proxy stopped");
                    break;
                }
            }
        }
    });
}

// ─── Local wire listeners (anti-DPI path) ─────────────────────────────
// Rewritten answers send the app to 127.0.0.1; these ports are where that
// traffic lands. Every one splices to the same port on the relay, so the
// relay and exit configs need no change at all.

/// One local port and its relay-side twin. `fragment` splits the first TLS
/// record (the ClientHello, the only one DPI can match SNI in) into 16-byte
/// pieces before splicing the rest untouched.
struct WirePort {
    listen: u16,
    relay_port: u16,
    fragment: bool,
}

/// 8443 is not optional: the panel host's public address *is* the relay
/// IP, so without a local 8443 the rewritten dashboard/API traffic would
/// dead-end at a port nothing is listening on.
const WIRE_PORTS: [WirePort; 4] = [
    WirePort { listen: 80, relay_port: 80, fragment: false },
    WirePort { listen: 443, relay_port: 443, fragment: true },
    WirePort { listen: 1119, relay_port: 1119, fragment: false },
    WirePort { listen: 8443, relay_port: 8443, fragment: true },
];

/// Bind every wire port or none of them. All-or-nothing on purpose: port
/// 80 or 443 is occasionally held by something else on a dev machine
/// (IIS, SQL Reporting), and a half-open set would rewrite answers into a
/// black hole. One busy port and the whole feature stays off — the proxy
/// then behaves exactly as it did before.
async fn bind_wire_ports() -> Result<Vec<(TcpListener, WirePort)>, String> {
    let mut bound = Vec::new();
    for port in WIRE_PORTS {
        match TcpListener::bind(("127.0.0.1", port.listen)).await {
            Ok(lst) => bound.push((lst, port)),
            Err(e) => {
                drop(bound);
                return Err(format!("local :{} busy ({e})", port.listen));
            }
        }
    }
    Ok(bound)
}

/// One accepted local connection: connect upstream first, optionally
/// re-send the client's first TLS record in fragments, then splice.
async fn handle_wire(mut client: TcpStream, mut up: TcpStream, fragment: bool) {
    up.set_nodelay(true).ok();
    if fragment {
        let mut hdr = [0u8; 5];
        let read = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            client.read_exact(&mut hdr),
        )
        .await;
        match read {
            // tokio's read_exact yields io::Result<usize>, not () like std's —
            // `Ok(Ok(()))` never matched and the whole fragment path was dead.
            Ok(Ok(_)) => match wireproxy::tls_record_payload(&hdr) {
                Some(len) => {
                    let mut body = vec![0u8; len];
                    if client.read_exact(&mut body).await.is_err() {
                        return;
                    }
                    if up.write_all(&hdr).await.is_err() {
                        return;
                    }
                    let mut at = 0usize;
                    for sz in wireproxy::fragment_sizes(len, 16) {
                        if up.write_all(&body[at..at + sz]).await.is_err() {
                            return;
                        }
                        at += sz;
                    }
                }
                None => {
                    // Not a splittable handshake record (TLS 1.3 compatibility
                    // cases, or something that is not TLS at all): the header
                    // is already consumed, so hand it back verbatim and splice.
                    if up.write_all(&hdr).await.is_err() {
                        return;
                    }
                }
            },
            _ => return,
        }
    }
    let _ = tokio::io::copy_bidirectional(&mut client, &mut up).await;
}

/// Accept loop for one wire port — same watch-based shutdown as the DNS
/// loops, so disconnect actually frees 80/443 for the next connect.
fn spawn_wire_loop(lst: TcpListener, up: SocketAddr, fragment: bool, mut stop: watch::Receiver<bool>) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                accepted = lst.accept() => {
                    match accepted {
                        Ok((client, _)) => {
                            tokio::spawn(async move {
                                match TcpStream::connect(up).await {
                                    Ok(upstream) => handle_wire(client, upstream, fragment).await,
                                    Err(e) => eprintln!("[PeDitXCDN] wire upstream {up}: {e}"),
                                }
                            });
                        }
                        Err(e) => {
                            eprintln!("[PeDitXCDN] wire accept error: {e}");
                            break;
                        }
                    }
                }
                _ = stop.changed() => break,
            }
        }
    });
}

/// Take 127.0.0.1:53 (UDP + TCP), retrying a transient AddrInUse for ~3 s.
///
/// A cancelled press unwinds on its own task, and between the bump and its
/// sockets being dropped sits a netsh loop undoing the repoint. Pressing
/// Connect inside that window used to fail outright with «پورت 53 اشغال
/// است» — a condition that lasts about a second, reported as if it were
/// permanent, which is what made a cancel look like it broke Connect for
/// good. `port_holder` (netstat + tasklist, ~0.5 s) only runs on the last
/// try; every earlier one is just another 100 ms.
async fn bind53() -> Result<(UdpSocket, TcpListener), String> {
    const ATTEMPTS: u32 = 30;
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let give_up = attempt + 1 == ATTEMPTS;

        let udp = match UdpSocket::bind("127.0.0.1:53").await {
            Ok(s) => s,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::AddrInUse && !give_up {
                    continue;
                }
                return Err(bind_error("UDP", e));
            }
        };
        match TcpListener::bind("127.0.0.1:53").await {
            Ok(tcp) => return Ok((udp, tcp)),
            Err(e) => {
                let transient = e.kind() == std::io::ErrorKind::AddrInUse;
                drop(udp);
                if transient && !give_up {
                    continue;
                }
                return Err(bind_error("TCP", e));
            }
        }
    }
    // Unreachable — the final attempt always returns from inside the loop.
    Err(bind_error("UDP", std::io::Error::from(std::io::ErrorKind::AddrInUse)))
}

/// Start the DNS proxy as a background tokio task.
/// Binds UDP+TCP on 127.0.0.1:53 and [::1]:53, forwards to relay_ip:53,
/// then changes system DNS to 127.0.0.1 / ::1.
async fn start_proxy(relay_ip: String) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    // Snapshot of "who pressed Cancel": every later Disconnect bumps it, so a
    // press abandoned mid-flight is recognised by every check below — and the
    // *next* connect takes a fresh snapshot and is unaffected.
    let gen = CANCEL_GEN.load(Ordering::SeqCst);
    // Stop any existing proxy first — *without* the DHCP round trip: we are
    // about to repoint DNS here anyway, and that loop is pure cost on a press.
    stop_proxy_inner(false).await;
    let t_stop = t0.elapsed();

    let relay_addr: SocketAddr = format!("{}:53", relay_ip)
        .parse()
        .map_err(|_| format!("invalid relay IP: {}", relay_ip))?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Loopback only: system DNS points at 127.0.0.1, so we never need
    // 0.0.0.0 — which both collides with ICS/Docker binds on other local
    // addresses and exposes an open resolver to the whole LAN.
    let (udp, tcp) = bind53().await?;

    // Second stack for the interface's IPv6 resolvers. A failure here is
    // survivable: set_system_dns then puts IPv6 back on dhcp rather than
    // pointing it at a dead ::1.
    let udp6 = UdpSocket::bind("[::1]:53").await.ok();
    let tcp6 = TcpListener::bind("[::1]:53").await.ok();

    // Local wire listeners — bound *before* system DNS switches, so a busy
    // port is discovered while nothing has been pointed at us yet.
    let wire = match bind_wire_ports().await {
        Ok(v) => v,
        Err(e) => {
            crate::api::log_to_file(&format!("WIREPROXY OFF: {e}"));
            eprintln!("[PeDitXCDN] wire listeners off: {e}");
            Vec::new()
        }
    };
    let relay_v4 = match relay_addr.ip() {
        std::net::IpAddr::V4(v4) => Some(v4.octets()),
        std::net::IpAddr::V6(_) => None,
    };
    // Rewrite only when the local listeners that consume the rewritten
    // address actually came up — otherwise 127.0.0.1:443 would be a hole.
    let rewrite = match (wire.is_empty(), relay_v4) {
        (false, Some(v4)) => Some(v4),
        _ => None,
    };

    let t_bind = t0.elapsed();

    // Change system DNS to localhost — off the runtime thread: the netsh
    // loop is several blocking spawns (~0.5s each) and awaiting it inline
    // starved the runtime that also drives the window.
    let proxy_v6 = udp6.is_some() && tcp6.is_some();
    // Nothing has been pointed at us yet, so an abandoned press can just drop
    // the sockets it bound — no restore to pay for.
    if let Err(e) = cancelled(gen) {
        crate::api::log_to_file(&format!("CONNECT ABORT before repoint: {e}"));
        return Err(e);
    }
    let repointed = tokio::task::spawn_blocking(move || set_system_dns("127.0.0.1", proxy_v6, gen))
        .await
        .map_err(|e| e.to_string())?;
    if let Err(e) = repointed {
        // The pre-bind restore this replaced was the only thing standing
        // between a failed repoint and a system left on a dead 127.0.0.1.
        // A cancel found inside the netsh loop lands here too.
        // Sockets out first: `restore` is seconds of netsh, and a press that
        // arrives while we still hold :53 would bind-fail against ourselves.
        drop((udp, tcp, udp6, tcp6, wire));
        let _ = tokio::task::spawn_blocking(restore_system_dns).await;
        crate::api::log_to_file(&format!("CONNECT ABORT after repoint: {e}"));
        return Err(e);
    }
    let t_dns = t0.elapsed();

    // Where a slow press actually spent its time. The SLOW lines above name
    // the individual netsh; this one says which phase to look at first.
    crate::api::log_to_file(&format!(
        "CONNECT stop={}ms bind={}ms dns={}ms v6={}",
        t_stop.as_millis(),
        t_bind.saturating_sub(t_stop).as_millis(),
        t_dns.saturating_sub(t_bind).as_millis(),
        proxy_v6,
    ));

    // Last chance to abandon the press: everything below publishes the proxy
    // as live, and DNS already points at it — so this is the one check that
    // has to undo the repoint itself. Sockets before the restore, same as
    // above: dropping them costs nothing, holding them costs the next press.
    if let Err(e) = cancelled(gen) {
        drop((udp, tcp, udp6, tcp6, wire));
        let _ = tokio::task::spawn_blocking(restore_system_dns).await;
        crate::api::log_to_file(&format!("CONNECT ABORT before publish: {e}"));
        return Err(e);
    }

    eprintln!(
        "[PeDitXCDN] DNS proxy started: 127.0.0.1:53{} -> {}:53 (fragment: {})",
        if udp6.is_some() { " + [::1]:53" } else { "" },
        relay_ip,
        if rewrite.is_some() { "on" } else { "off" },
    );

    crate::api::log_to_file(&format!(
        "PROXY START 127.0.0.1:53{} -> {relay_ip}:53 fragment={}",
        if proxy_v6 { " + [::1]:53" } else { "" },
        if rewrite.is_some() { "on" } else { "off" },
    ));

    // Store shutdown sender
    *SHUTDOWN.lock().unwrap() = Some(shutdown_tx);
    *LAST_RELAY.lock().unwrap() = Some(relay_ip.clone());
    *PROXY_STATE.lock().unwrap() = Some(ProxyState {
        relay: relay_ip.clone(),
        started: std::time::Instant::now(),
        v6: proxy_v6,
        fragment: rewrite.is_some(),
    });

    spawn_tcp_loop(tcp, relay_addr, rewrite, shutdown_rx.clone());
    spawn_udp_loop(std::sync::Arc::new(udp), relay_addr, rewrite, shutdown_rx.clone());
    if let Some(s) = udp6 {
        spawn_udp_loop(std::sync::Arc::new(s), relay_addr, rewrite, shutdown_rx.clone());
    }
    if let Some(l) = tcp6 {
        spawn_tcp_loop(l, relay_addr, rewrite, shutdown_rx.clone());
    }
    for (lst, port) in wire {
        let up: SocketAddr = (relay_addr.ip(), port.relay_port).into();
        spawn_wire_loop(lst, up, port.fragment, shutdown_rx.clone());
    }
    drop(shutdown_rx);

    Ok(())
}

/// Stop the proxy, and restore system DNS only when the caller asked for it.
///
/// `restore_dns` splits the two callers that used to share one behaviour:
/// Disconnect/emergency want DHCP back, while **start** is about to point DNS
/// at this very proxy again — so it used to pay a full multi-interface netsh
/// loop (candidates + one `set dns` per interface + ipv6 + flushdns, twice
/// counting the repoint that follows) on every single press. On a box where a
/// netsh call takes seconds that chain is what made Connect take minutes.
/// The safety net the pre-restore used to give is now in `start_proxy`, which
/// restores if the repoint itself fails.
async fn stop_proxy_inner(restore_dns: bool) {
    // Send shutdown signal (sync — watch::Sender::send needs no await)
    let tx = SHUTDOWN.lock().unwrap().take();
    let had_proxy = tx.is_some();
    // Cleared unconditionally: a stop means "not running", even when the
    // sender was already gone (a crash left no sender but did leave state).
    *PROXY_STATE.lock().unwrap() = None;
    if let Some(tx) = &tx {
        let _ = tx.send(true);
        // Wait briefly for tasks to exit
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    if !restore_dns {
        return;
    }

    // Restore only when there is something to restore from.
    let need = had_proxy || dns_points_at_proxy().await;
    if need {
        // Blocking netsh must not sit on an async worker.
        let _ = tokio::task::spawn_blocking(restore_system_dns).await;
        eprintln!("[PeDitXCDN] System DNS restored to DHCP");
    }
}

/// True when a status snapshot still points the system at our own proxy.
/// One definition for the three places that need it (the emergency cut, the
/// connect-time check below, and the startup stale-DNS sweep in lib.rs): a
/// mismatch between them is exactly how a killed session stayed pointed at a
/// dead 127.0.0.1 and broke the next login.
pub fn points_at_proxy(st: &DnsStatus) -> bool {
    st.current_dns.as_deref() == Some("127.0.0.1") || st.ipv6_dns.as_deref() == Some("::1")
}

/// True when the system is pointed at our own (now possibly dead) proxy.
async fn dns_points_at_proxy() -> bool {
    let st = tokio::task::spawn_blocking(get_dns_status)
        .await
        .ok()
        .and_then(|r| r.ok());
    matches!(st, Some(s) if points_at_proxy(&s))
}

// ─── Public API (called from Tauri commands) ──────────────────────────

/// Start the DNS proxy + change system DNS.
/// Async: `connect` awaits this instead of `handle().block_on()`. Blocking
/// the runtime from a command made the window sit unresponsive for the whole
/// netsh loop — Windows reported "Not responding" on the Connect press.
pub async fn start_dns_proxy(relay_ip: &str) -> Result<(), String> {
    start_proxy(relay_ip.to_string()).await
}

/// Stop the DNS proxy + restore system DNS (non-blocking, safe from any thread).
pub fn stop_dns_proxy() {
    // Also abandons a connect that is still mid-flight — same intent as the
    // async form below.
    bump_cancel();
    // Send the shutdown signal instead of just dropping the sender —
    // dropping stopped the UDP loop but left the TCP listener bound.
    if let Some(tx) = SHUTDOWN.lock().unwrap().take() {
        let _ = tx.send(true);
        // Give the accept/recv loops time to drop their sockets. Without
        // this an immediate reconnect raced them and bind :53 failed with
        // "port 53 is in use" — our own listener from a moment ago.
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    *PROXY_STATE.lock().unwrap() = None;
    // Restore DNS directly (sync, no tokio needed)
    let _ = restore_system_dns();
    eprintln!("[PeDitXCDN] DNS proxy stopped, system DNS restored to DHCP");
}

/// Cheap, in-memory service status — no netsh, no sockets. Safe to poll
/// every second; this is what the UI's «connected» actually follows now.
pub fn proxy_status() -> ProxyStatus {
    let st = PROXY_STATE.lock().unwrap();
    match &*st {
        Some(s) => ProxyStatus {
            running: true,
            relay: Some(s.relay.clone()),
            uptime_secs: s.started.elapsed().as_secs(),
            v6: s.v6,
            fragment: s.fragment,
        },
        None => ProxyStatus {
            running: false,
            relay: None,
            uptime_secs: 0,
            v6: false,
            fragment: false,
        },
    }
}

/// Relay the last successful start used — the tray's restart needs it
/// without asking the webview.
pub fn last_relay() -> Option<String> {
    LAST_RELAY.lock().unwrap().clone()
}

/// Emergency cut: stop every loop, then force DHCP back on both stacks
/// whether or not the proxy believed it was running. Certainty beats the
/// v0.3.18 idle fast-path here — this is the button pressed *because* DNS
/// is already broken, so it must not skip the restore to save 0.5 s.
/// Blocking (~1–2 s of netsh): the command runs it on a worker thread, the
/// tray on a detached one; neither may sit on the core thread.
pub fn emergency_stop() -> EmergencyStop {
    // A connect in flight must not finish behind this button either.
    bump_cancel();
    let was_running = SHUTDOWN.lock().unwrap().take().is_some();
    *PROXY_STATE.lock().unwrap() = None;
    // Let the accept/recv loops drop their sockets before anyone rebinds :53.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let _ = restore_system_dns();

    // Report what the system *says*, not what we intended: a netsh that
    // failed mid-loop leaves 127.0.0.1 pointing at a dead proxy, and the UI
    // has to be able to see that instead of showing a green tick. An
    // unreadable system counts as *not* verified — a green tick we did not
    // witness is worse than a warning.
    let st = get_dns_status().ok();
    let dns_restored = match &st {
        None => false,
        Some(s) => !points_at_proxy(s),
    };
    let (current_dns, ipv6_dns) = match st {
        Some(s) => (s.current_dns, s.ipv6_dns),
        None => (None, None),
    };

    let _ = crate::api::log_to_file(&format!(
        "EMERGENCY STOP running={was_running} dns={} ipv6={} restored={dns_restored}",
        current_dns.as_deref().unwrap_or("?"),
        ipv6_dns.as_deref().unwrap_or("?"),
    ));
    eprintln!(
        "[PeDitXCDN] emergency stop: running={was_running} restored={dns_restored}"
    );

    EmergencyStop {
        proxy_was_running: was_running,
        dns_restored,
        current_dns,
        ipv6_dns,
    }
}

/// Stop the DNS proxy + restore system DNS (async version for non-tokio threads).
pub async fn stop_dns_proxy_async() {
    // Disconnect is also the Cancel of an in-flight connect: the running
    // `start_proxy` sees the bump at its next phase boundary and unwinds.
    bump_cancel();
    stop_proxy_inner(true).await;
}

/// Get current DNS configuration status.
/// Every IP a netsh `show dns` block lists as a server.
/// The old filter skipped any line containing "dns servers" — which is the
/// *only* line that carries an address (`Statically Configured DNS Servers:
/// 127.0.0.1`) — so `current_dns` was always `None`, the startup recovery
/// never fired, and after a killed session the system stayed pointed at a
/// dead 127.0.0.1: login then failed with "error sending request for url".
/// Parsing tokens as IPs sidesteps the localized column names entirely.
fn netsh_dns_ips(text: &str) -> Vec<String> {
    let mut ips: Vec<String> = Vec::new();
    for tok in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        if let Ok(ip) = tok.parse::<std::net::IpAddr>() {
            let s = ip.to_string();
            if !ips.contains(&s) {
                ips.push(s);
            }
        }
    }
    ips
}

pub fn get_dns_status() -> Result<DnsStatus, String> {
    #[cfg(target_os = "windows")]
    {
        let iface = get_active_interface()?;
        let output = run("netsh", &["interface", "ip", "show", "dns", iface.as_str()])
            .map_err(|e| format!("failed to run netsh: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let configured = stdout.to_lowercase().contains("static");

        // Loopback first: a primary+secondary pair is still "proxy mode"
        // if either points at us.
        let v4 = netsh_dns_ips(&stdout);
        let current_dns = v4
            .iter()
            .find(|s| *s == "127.0.0.1")
            .cloned()
            .or_else(|| v4.first().cloned());

        // v0.3.15 also repoints the IPv6 resolvers at ::1 — a leftover ::1
        // after a kill is exactly as fatal as a leftover 127.0.0.1, and
        // Windows will happily keep racing it (SMHNR).
        let v6 = run("netsh", &["interface", "ipv6", "show", "dns", iface.as_str()])
            .ok()
            .map(|o| netsh_dns_ips(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default();
        let ipv6_dns = v6
            .iter()
            .find(|s| *s == "::1")
            .cloned()
            .or_else(|| v6.first().cloned());

        let is_relay_dns = current_dns.as_deref() == Some("127.0.0.1");

        Ok(DnsStatus {
            configured,
            current_dns,
            ipv6_dns,
            interface: iface,
            is_relay_dns,
        })
    }

    #[cfg(not(target_os = "windows"))]
    Ok(DnsStatus {
        configured: false,
        current_dns: None,
        ipv6_dns: None,
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
    let t0 = std::time::Instant::now();
    let host = reqwest::Url::parse(panel_url)
        .map_err(|e| format!("invalid panel URL: {e}"))?
        .host_str()
        .ok_or_else(|| "panel URL has no host".to_string())?
        .to_string();
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        return Ok(host);
    }
    // While connected, the system resolver *is* our proxy, and the rewrite
    // turns the panel host's answer into 127.0.0.1 — adopting that would
    // point the relay at itself. Any non-loopback answer wins outright; if
    // every answer is loopback the rewrite did it, so ask upstream directly.
    let local: Vec<std::net::Ipv4Addr> = (host.as_str(), 443u16)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve panel host {host}: {e}"))?
        .filter_map(|addr| match addr.ip() {
            std::net::IpAddr::V4(v4) => Some(v4),
            std::net::IpAddr::V6(_) => None,
        })
        .collect();
    // No timeout around getaddrinfo: a system resolver that is slow or dead
    // stalls this sync command (and the window with it) for as long as
    // Windows feels like. Say so in the log instead of guessing later.
    let ms = t0.elapsed().as_millis();
    if ms > 400 {
        let _ = crate::api::log_to_file(&format!(
            "SLOW {ms}ms resolve {host} -> {} addrs",
            local.len()
        ));
    }
    for v4 in &local {
        if !wireproxy::is_loopback_v4(v4.octets()) {
            return Ok(v4.to_string());
        }
    }
    // ponytail: two fixed public resolvers; a per-config upstream list is
    // the upgrade if both ever get filtered on a given network.
    for resolver in ["8.8.8.8:53", "1.1.1.1:53"] {
        if let Ok(ips) = query_addrs(resolver, &host, QTYPE_A) {
            for ip in ips {
                if let Ok(v4) = ip.parse::<std::net::Ipv4Addr>() {
                    if !wireproxy::is_loopback_v4(v4.octets()) {
                        return Ok(v4.to_string());
                    }
                }
            }
        }
    }
    local
        .first()
        .map(|v4| v4.to_string())
        .ok_or_else(|| format!("no IPv4 address for {host}"))
}

fn build_query(domain: &str, qtype: u16) -> Vec<u8> {
    let mut q = Vec::with_capacity(domain.len() + 18);
    q.extend_from_slice(&[0x37, 0x11]); // id — one outstanding query per socket
    q.extend_from_slice(&[0x01, 0x00]); // RD=1
    q.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]);
    for label in domain.trim_end_matches('.').split('.') {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0);
    q.extend_from_slice(&qtype.to_be_bytes());
    q.extend_from_slice(&[0x00, 0x01]); // IN
    q
}

/// Addresses of `qtype` from a response; maps the RCODE to an error so a
/// SERVFAIL from a dead relay is not reported as "no answer".
fn parse_answers(msg: &[u8], qtype: u16) -> Result<Vec<String>, String> {
    if msg.len() < 12 {
        return Err("پاسخ DNS ناقص است".to_string());
    }
    let rcode = u16::from_be_bytes([msg[2], msg[3]]) & 0x000F;
    if rcode != 0 {
        return Err(format!("پروکسی/رله rcode={rcode} برگرداند"));
    }
    let qd = u16::from_be_bytes([msg[4], msg[5]]) as usize;
    let an = u16::from_be_bytes([msg[6], msg[7]]) as usize;
    let mut i = 12usize;
    for _ in 0..qd {
        i = match skip_name(msg, i) {
            Some(x) => x + 4,
            None => return Err("پاسخ DNS ناقص است".to_string()),
        };
    }
    let mut ips: Vec<String> = Vec::new();
    for _ in 0..an {
        i = match skip_name(msg, i) {
            Some(x) => x,
            None => break,
        };
        if i + 10 > msg.len() {
            break;
        }
        let typ = u16::from_be_bytes([msg[i], msg[i + 1]]);
        let rdlen = u16::from_be_bytes([msg[i + 8], msg[i + 9]]) as usize;
        i += 10;
        if i + rdlen > msg.len() {
            break;
        }
        let rdata = &msg[i..i + rdlen];
        let ip = match (qtype, typ) {
            (QTYPE_A, 1) if rdlen == 4 => Some(std::net::IpAddr::V4(
                std::net::Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]),
            )),
            (QTYPE_AAAA, 28) if rdlen == 16 => {
                let mut o = [0u8; 16];
                o.copy_from_slice(rdata);
                Some(std::net::IpAddr::V6(std::net::Ipv6Addr::from(o)))
            }
            _ => None,
        };
        if let Some(ip) = ip {
            let s = ip.to_string();
            if !ips.contains(&s) {
                ips.push(s);
            }
        }
        i += rdlen;
    }
    Ok(ips)
}

/// One raw UDP query against a resolver. Sync — called from spawn_blocking.
fn query_addrs(server: &str, domain: &str, qtype: u16) -> Result<Vec<String>, String> {
    use std::net::UdpSocket;
    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("UDP bind: {e}"))?;
    sock.set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .map_err(|e| format!("timeout: {e}"))?;
    let q = build_query(domain, qtype);
    sock.send_to(&q, server)
        .map_err(|e| format!("ارسال کوئری DNS: {e}"))?;
    let mut buf = [0u8; 4096];
    let n = match sock.recv_from(&mut buf) {
        Ok((n, _)) => n,
        Err(e)
            if e.kind() == std::io::ErrorKind::TimedOut
                || e.kind() == std::io::ErrorKind::WouldBlock =>
        {
            return Err("پاسخی از پروکسی محلی نیامد (۲ ثانیه timeout)".to_string())
        }
        Err(e) => return Err(format!("خواندن پاسخ DNS: {e}")),
    };
    parse_answers(&buf[..n], qtype)
}

/// Resolve `domain` through the local proxy (127.0.0.1:53) with a raw query.
/// The visible proof that the connection is real: a hijacked domain answers
/// with the relay IP, a bypassed one with a public address, a dead proxy
/// times out. Replaces `nslookup`, whose reverse lookup of 127.0.0.1 and
/// localized output could not tell "no answer" from "answer we mis-parsed".
/// `relay` is optional: with it, a failed local query is retried straight
/// against the relay so the UI can say *which* hop is dead — "our proxy is
/// down" and "the network eats port 53" look identical otherwise, and the
/// next support round-trip costs an installer.
pub fn resolve_local(domain: &str, relay: Option<&str>) -> Result<LocalResolve, String> {
    let t0 = std::time::Instant::now();
    let a = match query_addrs("127.0.0.1:53", domain, QTYPE_A) {
        Ok(a) if !a.is_empty() => a,
        Ok(_) => {
            return Err(hop_diagnosis(
                "پاسخ A از پروکسی محلی برنگشت",
                domain,
                relay,
            ))
        }
        Err(e) => return Err(hop_diagnosis(&e, domain, relay)),
    };
    // Empty AAAA is the expected result while the proxy strips it
    // (nodata_aaaa) — the UI renders that as «مسدود».
    let aaaa = query_addrs("127.0.0.1:53", domain, QTYPE_AAAA).unwrap_or_default();
    Ok(LocalResolve {
        a,
        aaaa,
        ms: t0.elapsed().as_millis() as u64,
    })
}

/// Split "local proxy silent" into the two cases that need different fixes.
fn hop_diagnosis(proxy_err: &str, domain: &str, relay: Option<&str>) -> String {
    crate::api::log_to_file(&format!("PROBE FAIL local ({domain}): {proxy_err}"));
    let Some(relay) = relay else {
        return proxy_err.to_string();
    };
    let addr = format!("{relay}:53");
    match query_addrs(&addr, domain, QTYPE_A) {
        Ok(a) if !a.is_empty() => {
            crate::api::log_to_file(&format!("PROBE relay OK {addr}: {:?}", a));
            format!("رله {relay} سالم است؛ پروکسی محلی پاسخ نداد")
        }
        other => {
            crate::api::log_to_file(&format!(
                "PROBE relay {addr}: {}",
                other.err().unwrap_or_else(|| "empty A".to_string())
            ));
            format!("رله {relay} هم بی‌جواب — پورت 53 بسته است")
        }
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
