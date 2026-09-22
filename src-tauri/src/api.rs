use crate::types::{LoginResponse, PlansResponse, SimpleResponse, UserInfo};
use reqwest::Client;
use std::sync::OnceLock;

/// Log a message to %APPDATA%/PeDitXCDN/debug.log
pub fn log_to_file(msg: &str) {
    if let Some(dir) = dirs::data_dir() {
        let log_dir = dir.join("PeDitXCDN");
        let _ = std::fs::create_dir_all(&log_dir);
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("debug.log"))
            .unwrap();
        use std::io::Write;
        let _ = writeln!(f, "[{}] {}", chrono_wrapper(), msg);
    }
}

fn chrono_wrapper() -> String {
    format!("{:?}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs())
}

fn http_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            // ponytail: skip TLS verification for self-signed certs on panel.
            .danger_accept_invalid_certs(true)
            .cookie_store(true)
            .build()
            .expect("reqwest client")
    })
}

/// Login with username + password via the relay panel.
///
/// Sends JSON to POST /login. The relay detects Content-Type: application/json
/// and proxies to the exit's /user-password-login, returning JSON directly.
pub async fn login(
    panel_url: &str,
    username: &str,
    password: &str,
) -> Result<LoginResponse, String> {
    let url = format!("{}/login", panel_url);
    log_to_file(&format!("LOGIN: url={}, user={}", url, username));

    let resp = match http_client()
        .post(&url)
        .json(&serde_json::json!({"username": username, "password": password}))
        .send()
        .await {
            Ok(r) => r,
            Err(e) => {
                log_to_file(&format!("LOGIN ERROR: {}", e));
                return Err(format!("login request failed: {e}"));
            }
        };

    let status = resp.status();
    log_to_file(&format!("LOGIN STATUS: {}", status));
    let body = resp.text().await.unwrap_or_default();
    log_to_file(&format!("LOGIN BODY: {}", &body[..body.len().min(500)]));

    // JSON response from relay: {"ok": true, "session": "..."} or {"ok": false, "message": "..."}
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        let ok = json["ok"].as_bool().unwrap_or(false);
        let session = json["session"].as_str().map(|s| s.to_string());
        let message = json["message"].as_str().map(|s| s.to_string());
        log_to_file(&format!("LOGIN RESULT: ok={}, session={:?}, msg={:?}", ok, session, message));
        return Ok(LoginResponse { ok, session, message });
    }

    log_to_file(&format!("LOGIN UNEXPECTED: status={}", status));
    Ok(LoginResponse {
        ok: false,
        session: None,
        message: Some(format!("unexpected response: {}", status)),
    })
}

/// Extract session token from Set-Cookie header named "sdu".
fn extract_session_from_cookie(resp: &reqwest::Response) -> Option<String> {
    for value in resp.headers().get_all("set-cookie").iter() {
        if let Ok(s) = value.to_str() {
            if let Some(cookie) = s.split(';').next() {
                if let Some((name, val)) = cookie.split_once('=') {
                    if name.trim() == "sdu" {
                        return Some(val.trim().to_string());
                    }
                }
            }
        }
    }
    None
}

/// Create a new account via JSON API.
pub async fn signup(
    panel_url: &str,
    username: &str,
    password: &str,
    name: &str,
) -> Result<LoginResponse, String> {
    let url = format!("{}/signup", panel_url);
    log_to_file(&format!("SIGNUP: url={}, user={}", url, username));

    let resp = match http_client()
        .post(&url)
        .json(&serde_json::json!({
            "username": username,
            "password": password,
            "name": name,
        }))
        .send()
        .await {
            Ok(r) => r,
            Err(e) => {
                log_to_file(&format!("SIGNUP ERROR: {}", e));
                return Err(format!("signup request failed: {e}"));
            }
        };

    let body = resp.text().await.unwrap_or_default();
    log_to_file(&format!("SIGNUP BODY: {}", &body[..body.len().min(500)]));

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        let ok = json["ok"].as_bool().unwrap_or(false);
        let session = json["session"].as_str().map(|s| s.to_string());
        let message = json["message"].as_str().map(|s| s.to_string());
        return Ok(LoginResponse { ok, session, message });
    }

    Ok(LoginResponse { ok: false, session: None, message: Some("signup failed".into()) })
}

/// Fetch user info (quota, plan, speed, expiry, etc.) via JSON API.
pub async fn get_user_info(panel_url: &str, session: &str) -> Result<UserInfo, String> {
    let url = format!("{}/user-info", panel_url);
    let resp = http_client()
        .post(&url)
        .json(&serde_json::json!({"session": session}))
        .send()
        .await
        .map_err(|e| format!("user-info request failed: {e}"))?;
    let body = resp.text().await.unwrap_or_default();
    log_to_file(&format!("USER_INFO BODY: {}", &body[..body.len().min(1000)]));
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        if !json["ok"].as_bool().unwrap_or(false) {
            return Err(json["message"].as_str().unwrap_or("user info failed").to_string());
        }
        return Ok(UserInfo {
            ok: true,
            name: json["name"].as_str().map(|s| s.to_string()),
            telegram_id: json["telegram_id"].as_i64(),
            ip: json["ip"].as_str().map(|s| s.to_string()),
            used: json["used"].as_f64(),
            quota: json["quota"].as_f64(),
            status: json["status"].as_str().map(|s| s.to_string()),
            wallet: json["wallet"].as_f64(),
            plan: json["plan"].as_str().map(|s| s.to_string()),
            plan_name: json["plan_name"].as_str().map(|s| s.to_string()),
            renews: json["renews"].as_str().map(|s| s.to_string()),
            expires: json["expires"].as_str().map(|s| s.to_string()),
            speed_kbps: json["speed_kbps"].as_f64(),
            speed_mbps: json["speed_mbps"].as_f64(),
            days_left: json["days_left"].as_i64().map(|v| v as i32),
            gb_used: json["gb_used"].as_f64(),
            gb_total: json["gb_total"].as_f64(),
            warned: json["warned"].as_i64().map(|v| v as i32),
            seen_ip: json["seen_ip"].as_str().map(|s| s.to_string()),
        });
    }
    Err("invalid user-info response".into())
}

/// Fetch available plans via JSON API.
pub async fn get_plans(panel_url: &str, session: &str) -> Result<PlansResponse, String> {
    let url = format!("{}/plans", panel_url);
    let resp = http_client()
        .post(&url)
        .json(&serde_json::json!({"session": session}))
        .send()
        .await
        .map_err(|e| format!("plans request failed: {e}"))?;
    let body = resp.text().await.unwrap_or_default();
    log_to_file(&format!("PLANS BODY: {}", &body[..body.len().min(1000)]));
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        if !json["ok"].as_bool().unwrap_or(false) {
            return Err(json["message"].as_str().unwrap_or("plans failed").to_string());
        }
        let plans = json["plans"].as_array().map(|arr| arr.iter().filter_map(|p| Some(crate::types::Plan {
            id: p["id"].as_i64()? as i32,
            name: p["name"].as_str()?.to_string(),
            price: p["price"].as_i64().unwrap_or(0),
            desc: p["desc"].as_str().map(|s| s.to_string()),
            days: p["days"].as_i64().map(|v| v as i32),
            gb: p["gb"].as_f64(),
            mbps: p["mbps"].as_f64(),
        })).collect());
        let current = json["current"].as_i64().map(|v| v as i32);
        return Ok(PlansResponse { ok: true, plans, current });
    }
    Err("invalid plans response".into())
}

/// Register/update the client's IP address with the panel.
pub async fn claim_ip(panel_url: &str, session: &str, ip: &str) -> Result<SimpleResponse, String> {
    let url = format!("{}/register-ip", panel_url);
    log_to_file(&format!("CLAIM_IP: url={}, ip={}", url, ip));

    let resp = http_client()
        .post(&url)
        .json(&serde_json::json!({"session": session, "ip": ip}))
        .send()
        .await
        .map_err(|e| format!("claim-ip request failed: {e}"))?;

    let body = resp.text().await.unwrap_or_default();
    log_to_file(&format!("CLAIM_IP BODY: {}", &body[..body.len().min(500)]));

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        let ok = json["ok"].as_bool().unwrap_or(false);
        let message = json["message"].as_str().map(|s| s.to_string());
        return Ok(SimpleResponse { ok, message });
    }

    Ok(SimpleResponse { ok: false, message: Some("claim failed".into()) })
}
