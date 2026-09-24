use serde::{Deserialize, Serialize};

// --- Panel API responses ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub ok: bool,
    pub session: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub ok: bool,
    pub name: Option<String>,
    pub telegram_id: Option<i64>,
    pub ip: Option<String>,
    pub used: Option<f64>,
    pub quota: Option<f64>,
    pub status: Option<String>,
    pub wallet: Option<f64>,
    pub plan: Option<String>,
    pub plan_name: Option<String>,
    pub renews: Option<String>,
    pub expires: Option<String>,
    pub speed_kbps: Option<f64>,
    pub speed_mbps: Option<f64>,
    pub days_left: Option<i32>,
    pub gb_used: Option<f64>,
    pub gb_total: Option<f64>,
    pub warned: Option<serde_json::Value>,
    pub seen_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: i32,
    pub name: String,
    pub price: i64,
    pub desc: Option<String>,
    pub days: Option<i32>,
    pub gb: Option<f64>,
    pub mbps: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlansResponse {
    pub ok: bool,
    pub plans: Option<Vec<Plan>>,
    pub current: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleResponse {
    pub ok: bool,
    pub message: Option<String>,
}

// --- Internal types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsStatus {
    pub configured: bool,
    pub current_dns: Option<String>,
    pub ipv6_dns: Option<String>,
    pub interface: String,
    pub is_relay_dns: bool,
}

/// Result of probing the local proxy: both record types are reported, not
/// just "some address", because A is the hijacked one and a live AAAA means
/// the browser can still leave the tunnel over IPv6.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalResolve {
    pub a: Vec<String>,
    pub aaaa: Vec<String>,
    pub ms: u64,
}

/// Backend's view of the proxy, straight from the running loops — no netsh,
/// so it can be polled every second without freezing anything.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub relay: Option<String>,
    pub uptime_secs: u64,
    pub v6: bool,
    pub fragment: bool,
}

/// What the emergency cut actually did, read back from the system rather
/// than assumed. `dns_restored` is *verified* restoration: false both when a
/// netsh failed and left 127.0.0.1 behind, and when the system could not be
/// read at all — the caller tells those apart by whether the addresses came
/// back (`None` addresses = unknown). Neither may be shown as success.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyStop {
    pub proxy_was_running: bool,
    pub dns_restored: bool,
    pub current_dns: Option<String>,
    pub ipv6_dns: Option<String>,
}
