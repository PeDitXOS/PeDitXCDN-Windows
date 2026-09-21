use crate::types::DnsStatus;
use std::process::Command;

/// Get the active network interface name via `netsh`.
#[cfg(target_os = "windows")]
fn get_active_interface() -> Result<String, String> {
    let output = Command::new("netsh")
        .args(["interface", "ip", "show", "interfaces"])
        .output()
        .map_err(|e| format!("failed to run netsh: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.to_lowercase().contains("connected") && !line.to_lowercase().contains("loopback") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let connected_idx = parts.iter().position(|&p| p.eq_ignore_ascii_case("connected"));
                if let Some(idx) = connected_idx {
                    if idx + 1 < parts.len() {
                        return Ok(parts[idx + 1..].join(" "));
                    }
                }
            }
        }
    }
    Ok("Ethernet".to_string())
}

/// Set DNS to the relay IP.
#[cfg(target_os = "windows")]
pub fn set_dns(relay_ip: &str) -> Result<(), String> {
    let iface = get_active_interface()?;
    let status = Command::new("netsh")
        .args(["interface", "ip", "set", "dns", &iface, "static", relay_ip])
        .output()
        .map_err(|e| format!("failed to run netsh: {e}"))?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(format!("netsh failed: {stderr}"));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn set_dns(_relay_ip: &str) -> Result<(), String> {
    Err("DNS management is only supported on Windows".into())
}

/// Restore DNS to DHCP (automatic).
#[cfg(target_os = "windows")]
pub fn restore_dns() -> Result<(), String> {
    let iface = get_active_interface()?;
    let status = Command::new("netsh")
        .args(["interface", "ip", "set", "dns", &iface, "dhcp"])
        .output()
        .map_err(|e| format!("failed to run netsh: {e}"))?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(format!("netsh failed: {stderr}"));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn restore_dns() -> Result<(), String> {
    Err("DNS management is only supported on Windows".into())
}

/// Get current DNS configuration status.
#[cfg(target_os = "windows")]
pub fn get_dns_status() -> Result<DnsStatus, String> {
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

    // Check if current DNS matches any relay IP (we don't hardcode one anymore)
    let is_relay_dns = configured && current_dns.is_some();

    Ok(DnsStatus {
        configured,
        current_dns,
        interface: iface,
        is_relay_dns,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn get_dns_status() -> Result<DnsStatus, String> {
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
