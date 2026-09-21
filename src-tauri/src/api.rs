use crate::types::{LoginResponse, PlansResponse, SimpleResponse, UserInfo};
use reqwest::Client;
use std::sync::OnceLock;

fn http_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            // ponytail: skip TLS verification for self-signed certs on panel.
            // Upgrade when panel gets a proper CA-signed cert.
            .danger_accept_invalid_certs(true)
            .build()
            .expect("reqwest client")
    })
}

/// Login with username + password. Returns session token on success.
pub async fn login(
    panel_url: &str,
    username: &str,
    password: &str,
) -> Result<LoginResponse, String> {
    let url = format!("{}/user-password-login", panel_url);
    let body = serde_json::json!({
        "username": username,
        "password": password,
    });
    let resp = http_client()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("login request failed: {e}"))?;
    resp.json().await.map_err(|e| format!("login parse failed: {e}"))
}

/// Create a new account.
pub async fn signup(
    panel_url: &str,
    username: &str,
    password: &str,
    name: &str,
) -> Result<LoginResponse, String> {
    let url = format!("{}/user-signup", panel_url);
    let body = serde_json::json!({
        "username": username,
        "password": password,
        "name": name,
    });
    let resp = http_client()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("signup request failed: {e}"))?;
    resp.json().await.map_err(|e| format!("signup parse failed: {e}"))
}

/// Fetch user info (quota, plan, speed, expiry, etc.).
pub async fn get_user_info(panel_url: &str, session: &str) -> Result<UserInfo, String> {
    let url = format!("{}/user-info", panel_url);
    let body = serde_json::json!({ "session": session });
    let resp = http_client()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("user-info request failed: {e}"))?;
    resp.json().await.map_err(|e| format!("user-info parse failed: {e}"))
}

/// Fetch available plans.
pub async fn get_plans(panel_url: &str, session: &str) -> Result<PlansResponse, String> {
    let url = format!("{}/plans", panel_url);
    let body = serde_json::json!({ "session": session });
    let resp = http_client()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("plans request failed: {e}"))?;
    resp.json().await.map_err(|e| format!("plans parse failed: {e}"))
}

/// Register/update the client's IP address with the panel.
pub async fn claim_ip(panel_url: &str, session: &str, ip: &str) -> Result<SimpleResponse, String> {
    let url = format!("{}/user-claim", panel_url);
    let body = serde_json::json!({ "session": session, "ip": ip });
    let resp = http_client()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("claim-ip request failed: {e}"))?;
    resp.json().await.map_err(|e| format!("claim-ip parse failed: {e}"))
}
