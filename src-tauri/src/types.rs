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
    pub interface: String,
    pub is_relay_dns: bool,
}
