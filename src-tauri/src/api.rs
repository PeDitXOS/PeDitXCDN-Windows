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

/// Fetch user info (quota, plan, speed, expiry, etc.).
pub async fn get_user_info(panel_url: &str, _session: &str) -> Result<UserInfo, String> {
    let url = format!("{}/", panel_url);
    let resp = http_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("user-info request failed: {e}"))?;
    let html = resp.text().await.map_err(|e| format!("user-info read failed: {e}"))?;
    parse_dashboard_html(&html)
}

/// Parse the dashboard HTML to extract user info.
fn parse_dashboard_html(html: &str) -> Result<UserInfo, String> {
    let get_val = |key: &str| -> Option<String> {
        let patterns = [format!("class='k'>{}</div>", key), format!("class=\"k\">{}</div>", key)];
        for pat in &patterns {
            if let Some(pos) = html.find(pat.as_str()) {
                let after = &html[pos + pat.len()..];
                if let Some(start) = after.find("class='v'>") {
                    let val = &after[start + 10..];
                    if let Some(end) = val.find('<') {
                        return Some(val[..end].to_string());
                    }
                }
            }
        }
        None
    };

    Ok(UserInfo {
        ok: true,
        name: get_val("نام"),
        ip: None,
        telegram_id: None,
        used: None,
        quota: None,
        status: get_val("وضعیت").map(|s| if s.contains("فعال") || s.contains("active") { "active".into() } else { s }),
        wallet: None,
        plan: None,
        plan_name: get_val("plan"),
        renews: None,
        expires: get_val("انقضا").or_else(|| get_val("تاریخ انقضا")),
        speed_kbps: None,
        speed_mbps: get_val("سرعت").and_then(|s| s.replace("Kb/s", "").replace(" Mb/s", "").trim().parse().ok()),
        days_left: get_val("روز باقیمانده").and_then(|s| s.parse().ok()),
        gb_used: get_val("حجم مصرفی").and_then(|s| s.replace("GB", "").trim().parse().ok()),
        gb_total: get_val("حجم کل").and_then(|s| s.replace("GB", "").trim().parse().ok()),
        warned: None,
        seen_ip: None,
    })
}

/// Fetch available plans.
pub async fn get_plans(panel_url: &str, _session: &str) -> Result<PlansResponse, String> {
    let url = format!("{}/plans", panel_url);
    let resp = http_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("plans request failed: {e}"))?;
    let html = resp.text().await.map_err(|e| format!("plans read failed: {e}"))?;

    if let Ok(plans) = serde_json::from_str::<PlansResponse>(&html) {
        return Ok(plans);
    }

    let mut plans = Vec::new();
    for block in html.split("class='card'") {
        if block.contains("plan") || block.contains("پلن") {
            if let Some(name) = extract_between(block, "class='v'>", "<") {
                plans.push(crate::types::Plan {
                    id: plans.len() as i32 + 1,
                    name,
                    price: 0,
                    desc: None,
                    days: None,
                    gb: None,
                    mbps: None,
                });
            }
        }
    }
    Ok(PlansResponse { ok: true, plans: if plans.is_empty() { None } else { Some(plans) }, current: None })
}

fn extract_between<'a>(haystack: &'a str, start: &str, end: &str) -> Option<String> {
    let s = haystack.find(start)? + start.len();
    let e = haystack[s..].find(end)? + s;
    Some(haystack[s..e].trim().to_string())
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
