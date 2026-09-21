use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsStatus {
    pub configured: bool,
    pub current_dns: Option<String>,
    pub interface: String,
    pub is_relay_dns: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelData {
    pub relay_ip: String,
    pub relay_port: Option<u16>,
    pub panel_url: String,
    pub status: String,
    pub subscription_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)] // ponytail: reserved for relay health info in tray tooltip
pub struct RelayInfo {
    pub ip: String,
    pub port: u16,
    pub connected: bool,
    pub latency_ms: Option<u64>,
}
