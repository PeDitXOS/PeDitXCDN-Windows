use crate::types::DnsStatus;
use std::net::SocketAddr;
use std::process::Command;
use std::sync::Mutex;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Global shutdown sender — signals proxy tasks to stop.
/// Wrapped in Mutex<Option<>> so it can be replaced on each start.
static SHUTDOWN: Mutex<Option<mpsc::Sender<()>>> = Mutex::new(None);

// ─── System DNS helpers (netsh) ───────────────────────────────────────

#[cfg(target_os = "windows")]
fn get_active_interface() -> Result<String, String> {
    let output = Command::new("netsh")
        .args(["interface", "ip", "show", "interfaces"])
        .output()
        .map_err(|e| format!("failed to run netsh: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let lower = line.to_lowercase();
        if lower.contains("connected") && !lower.contains("loopback") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                if let Some(idx) = parts.iter().position(|&p| p.eq_ignore_ascii_case("connected")) {
                    if idx + 1 < parts.len() {
                        return Ok(parts[idx + 1..].join(" "));
                    }
                }
            }
        }
    }
    Ok("Ethernet".to_string())
}

#[cfg(not(target_os = "windows"))]
fn get_active_interface() -> Result<String, String> {
    Ok("eth0".into())
}

/// Set system DNS to a specific IP (used to point at our local proxy).
#[cfg(target_os = "windows")]
fn set_system_dns(dns_ip: &str) -> Result<(), String> {
    let iface = get_active_interface()?;
    let status = Command::new("netsh")
        .args(["interface", "ip", "set", "dns", &iface, "static", dns_ip])
        .output()
        .map_err(|e| format!("failed to run netsh: {e}"))?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(format!("netsh set dns failed: {stderr}"));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn set_system_dns(_dns_ip: &str) -> Result<(), String> {
    Ok(())
}

/// Restore system DNS to DHCP (automatic).
#[cfg(target_os = "windows")]
fn restore_system_dns() -> Result<(), String> {
    let iface = get_active_interface()?;
    let status = Command::new("netsh")
        .args(["interface", "ip", "set", "dns", &iface, "dhcp"])
        .output()
        .map_err(|e| format!("failed to run netsh: {e}"))?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(format!("netsh restore dns failed: {stderr}"));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn restore_system_dns() -> Result<(), String> {
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

/// Start the DNS proxy as a background tokio task.
/// Binds UDP+TCP on port 53, forwards to relay_ip:53.
/// Changes system DNS to 127.0.0.1.
async fn start_proxy(relay_ip: String) -> Result<(), String> {
    // Stop any existing proxy first
    stop_proxy_inner().await;

    let relay_addr: SocketAddr = format!("{}:53", relay_ip)
        .parse()
        .map_err(|_| format!("invalid relay IP: {}", relay_ip))?;

    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

    // Bind UDP socket
    let udp = UdpSocket::bind("0.0.0.0:53")
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                "Port 53 is in use. Stop the Windows DNS Client service or close other DNS software.".to_string()
            } else {
                format!("Failed to bind UDP port 53: {e}")
            }
        })?;
    let udp = std::sync::Arc::new(udp);

    // Bind TCP listener
    let tcp = TcpListener::bind("0.0.0.0:53")
        .await
        .map_err(|e| format!("Failed to bind TCP port 53: {e}"))?;

    // Change system DNS to localhost
    set_system_dns("127.0.0.1")?;

    eprintln!("[PeDitXCDN] DNS proxy started: 0.0.0.0:53 -> {}:53", relay_ip);

    // Store shutdown sender
    *SHUTDOWN.lock().unwrap() = Some(shutdown_tx);

    // Spawn TCP accept loop
    let tcp_relay = relay_addr;
    tokio::spawn(async move {
        loop {
            match tcp.accept().await {
                Ok((stream, _)) => {
                    let relay = tcp_relay;
                    tokio::spawn(async move {
                        handle_tcp(stream, relay).await;
                    });
                }
                Err(_) => break,
            }
        }
    });

    // Spawn UDP forwarding loop (non-blocking — returns immediately)
    let udp_clone = udp.clone();
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
                _ = shutdown_rx.recv() => {
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
    // Send shutdown signal
    let tx = SHUTDOWN.lock().unwrap().take();
    if let Some(tx) = tx {
        let _ = tx.send(()).await;
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
    let rt = tokio::runtime::Handle::current();
    rt.block_on(start_proxy(relay_ip.to_string()))
}

/// Stop the DNS proxy + restore system DNS (blocking version for commands).
pub fn stop_dns_proxy() {
    let rt = tokio::runtime::Handle::current();
    rt.block_on(stop_proxy_inner());
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
        let output = Command::new("netsh")
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

/// Ping the relay IP to check connectivity.
pub fn check_relay_connection(relay_ip: &str) -> Result<bool, String> {
    let output = if cfg!(target_os = "windows") {
        Command::new("ping")
            .args(["-n", "1", "-w", "1000", relay_ip])
            .output()
    } else {
        Command::new("ping")
            .args(["-c", "1", "-W", "1", relay_ip])
            .output()
    };

    let result = output.map_err(|e| format!("ping failed: {e}"))?;
    Ok(result.status.success())
}
