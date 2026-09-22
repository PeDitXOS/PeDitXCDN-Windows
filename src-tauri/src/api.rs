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
            // Follow redirects — the relay returns 303 with Set-Cookie after login.
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .expect("reqwest client")
    })
}

/// Login with username + password via the relay panel.
///
/// The relay's POST /login expects URL-encoded form data (username + password).
/// On success the relay returns 303 redirect to /register-ip with a Set-Cookie
/// header containing the session token. We follow the redirect, then extract
/// the session from the cookies jar.
pub async fn login(
    panel_url: &str,
    username: &str,
    password: &str,
) -> Result<LoginResponse, String> {
    let url = format!("{}/login", panel_url);
    log_to_file(&format!("LOGIN: url={}, user={}", url, username));

    let resp = match http_client()
        .post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
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

    // After following redirects, the final URL tells us whether login succeeded.
    // The relay redirects to /register-ip on success, or back to /login?m=...&e=1 on failure.
    let final_url = resp.url().clone().to_string();
    log_to_file(&format!("LOGIN FINAL URL: {}", final_url));

    // Check for error in redirect query params
    if final_url.contains("e=1") {
        if let Some(query) = final_url.split('?').nth(1) {
            for part in query.split('&') {
                if let Some((key, val)) = part.split_once('=') {
                    if key == "m" {
                        let msg = urlencoding::decode(val).unwrap_or_default().to_string();
                        log_to_file(&format!("LOGIN FAILED: {}", msg));
                        return Ok(LoginResponse {
                            ok: false,
                            session: None,
                            message: Some(msg),
                        });
                    }
                }
            }
        }
        log_to_file("LOGIN FAILED: redirect with e=1 but no message".into());
        return Ok(LoginResponse {
            ok: false,
            session: None,
            message: Some("login failed".into()),
        });
    }

    // Success — extract session token from cookies
    let session = extract_session_from_response(&resp);
    log_to_file(&format!("LOGIN SESSION: {:?}", session));

    if session.is_some() {
        Ok(LoginResponse {
            ok: true,
            session,
            message: None,
        })
    } else {
        // No session cookie — maybe we landed on the dashboard without redirect.
        // Check if the body looks like the dashboard (has account info).
        let body = resp.text().await.unwrap_or_default();
        log_to_file(&format!("LOGIN NO COOKIE body (500): {}", &body[..body.len().min(500)]));

        if body.contains("class='v'>") || body.contains("class=\"v\">") {
            // Looks like dashboard HTML — login succeeded but no cookie was set (weird).
            Ok(LoginResponse {
                ok: true,
                session: Some("cookie".into()),
                message: None,
            })
        } else {
            Ok(LoginResponse {
                ok: false,
                session: None,
                message: Some("login failed".into()),
            })
        }
    }
}

/// Extract the session token from the Set-Cookie header named "sdu".
fn extract_session_from_response(resp: &reqwest::Response) -> Option<String> {
    let headers = resp.headers();
    for value in headers.get_all("set-cookie").iter() {
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

/// Create a new account.
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
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[("username", username), ("password", password), ("name", name)])
        .send()
        .await {
            Ok(r) => r,
            Err(e) => {
                log_to_file(&format!("SIGNUP ERROR: {}", e));
                return Err(format!("signup request failed: {e}"));
            }
        };

    let status = resp.status();
    let final_url = resp.url().clone().to_string();
    log_to_file(&format!("SIGNUP STATUS: {}, FINAL: {}", status, final_url));

    if final_url.contains("e=1") {
        if let Some(query) = final_url.split('?').nth(1) {
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
        let session = extract_session_from_response(&resp);
        Ok(LoginResponse {
            ok: true,
            session: session.or(Some("cookie".into())),
            message: None,
        })
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
pub async fn claim_ip(panel_url: &str, _session: &str, ip: &str) -> Result<SimpleResponse, String> {
    let url = format!("{}/register-ip", panel_url);
    log_to_file(&format!("CLAIM_IP: url={}, ip={}", url, ip));

    let resp = http_client()
        .post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[("ip", ip)])
        .send()
        .await
        .map_err(|e| format!("claim-ip request failed: {e}"))?;

    let status = resp.status();
    let final_url = resp.url().clone().to_string();
    log_to_file(&format!("CLAIM_IP STATUS: {}, FINAL: {}", status, final_url));

    if final_url.contains("e=1") {
        Ok(SimpleResponse { ok: false, message: Some("claim failed".into()) })
    } else {
        Ok(SimpleResponse { ok: true, message: Some("IP claimed".into()) })
    }
}
