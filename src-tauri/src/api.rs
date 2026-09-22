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
            // Upgrade when panel gets a proper CA-signed cert.
            .danger_accept_invalid_certs(true)
            .cookie_store(true)
            .build()
            .expect("reqwest client")
    })
}

/// Login with username + password. Uses form-based login, returns session on success.
pub async fn login(
    panel_url: &str,
    username: &str,
    password: &str,
) -> Result<LoginResponse, String> {
    let url = format!("{}/login", panel_url);
    log_to_file(&format!("LOGIN: url={}, user={}", url, username));
    let resp = match http_client()
        .post(&url)
        .form(&[("username", username), ("password", password)])
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
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if status == 303 || status == 302 {
        log_to_file(&format!("LOGIN REDIRECT: location={}", location));
        // Decode redirect message
        if let Some(query) = location.split('?').nth(1) {
            for part in query.split('&') {
                if let Some((key, val)) = part.split_once('=') {
                    if key == "m" {
                        let msg = urlencoding::decode(val).unwrap_or_default().to_string();
                        let is_error = location.contains("e=1");
                        return Ok(LoginResponse {
                            ok: !is_error,
                            session: if is_error { None } else { Some("cookie".into()) },
                            message: Some(msg),
                        });
                    }
                }
            }
        }
        // Successful redirect (no error) — session is in cookies
        Ok(LoginResponse {
            ok: true,
            session: Some("cookie".into()),
            message: None,
        })
    } else if status == 200 {
        // Panel returned 200 directly (some panels don't redirect on login)
        let body = resp.text().await.unwrap_or_default();
        log_to_file(&format!("LOGIN 200 body (500): {}", &body[..body.len().min(500)]));
        let is_error = body.contains("class=\"error\"")
            || body.contains("e=1")
            || body.contains("error")
            || body.contains("خطا");
        if is_error {
            Ok(LoginResponse {
                ok: false,
                session: None,
                message: Some("login failed".into()),
            })
        } else {
            Ok(LoginResponse {
                ok: true,
                session: Some("cookie".into()),
                message: None,
            })
        }
    } else {
        log_to_file(&format!("LOGIN UNEXPECTED: status={}", status));
        Ok(LoginResponse {
            ok: false,
            session: None,
            message: Some(format!("unexpected status: {}", status)),
        })
    }
}

/// Create a new account.
pub async fn signup(
    panel_url: &str,
    username: &str,
    password: &str,
    name: &str,
) -> Result<LoginResponse, String> {
    let url = format!("{}/signup", panel_url);
    let resp = http_client()
        .post(&url)
        .form(&[
            ("username", username),
            ("password", password),
            ("name", name),
        ])
        .send()
        .await
        .map_err(|e| format!("signup request failed: {e}"))?;

    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if location.contains("e=1") {
        if let Some(query) = location.split('?').nth(1) {
            for part in query.split('&') {
                if let Some((key, val)) = part.split_once('=') {
                    if key == "m" {
                        let msg = urlencoding::decode(val).unwrap_or_default().to_string();
                        return Ok(LoginResponse { ok: false, session: None, message: Some(msg) });
                    }
                }
            }
        }
        Ok(LoginResponse { ok: false, session: None, message: Some("signup failed".into()) })
    } else {
        Ok(LoginResponse { ok: true, session: Some("cookie".into()), message: None })
    }
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
    // Parse the HTML dashboard to extract user info
    parse_dashboard_html(&html)
}

/// Parse the dashboard HTML to extract user info.
fn parse_dashboard_html(html: &str) -> Result<UserInfo, String> {
    // The dashboard has rows with key-value pairs
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

    // Try JSON first
    if let Ok(plans) = serde_json::from_str::<PlansResponse>(&html) {
        return Ok(plans);
    }

    // Parse HTML plans page
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
pub async fn claim_ip(panel_url: &str, _session: &str, ip: &str) -> Result<SimpleResponse, String> {
    let url = format!("{}/", panel_url);
    let resp = http_client()
        .post(&url)
        .form(&[("ip", ip)])
        .send()
        .await
        .map_err(|e| format!("claim-ip request failed: {e}"))?;
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if location.contains("e=1") {
        Ok(SimpleResponse { ok: false, message: Some("claim failed".into()) })
    } else {
        Ok(SimpleResponse { ok: true, message: Some("IP claimed".into()) })
    }
}
