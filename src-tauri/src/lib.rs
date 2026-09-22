mod api;
mod dns;
mod types;

use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use types::{DnsStatus, LocalResolve, LoginResponse, PlansResponse, SimpleResponse, UserInfo};

struct AppState {
    connected: Mutex<bool>,
    session: Mutex<Option<String>>,
    panel_url: Mutex<Option<String>>,
}

#[tauri::command]
async fn login(
    state: tauri::State<'_, AppState>,
    panel_url: String,
    username: String,
    password: String,
) -> Result<LoginResponse, String> {
    let resp = api::login(&panel_url, &username, &password).await?;
    if resp.ok {
        if let Some(ref token) = resp.session {
            *state.session.lock().unwrap() = Some(token.clone());
            *state.panel_url.lock().unwrap() = Some(panel_url);
        }
    }
    Ok(resp)
}

#[tauri::command]
async fn signup(
    panel_url: String,
    username: String,
    password: String,
    name: String,
) -> Result<LoginResponse, String> {
    api::signup(&panel_url, &username, &password, &name).await
}

#[tauri::command]
async fn get_user_info(
    state: tauri::State<'_, AppState>,
    panel_url: Option<String>,
    session: Option<String>,
) -> Result<UserInfo, String> {
    let url = panel_url
        .or_else(|| state.panel_url.lock().unwrap().clone())
        .ok_or("panel URL not set")?;
    let tok = session
        .or_else(|| state.session.lock().unwrap().clone())
        .ok_or("not logged in")?;
    api::get_user_info(&url, &tok).await
}

#[tauri::command]
async fn get_plans(
    state: tauri::State<'_, AppState>,
    panel_url: Option<String>,
    session: Option<String>,
) -> Result<PlansResponse, String> {
    let url = panel_url
        .or_else(|| state.panel_url.lock().unwrap().clone())
        .ok_or("panel URL not set")?;
    let tok = session
        .or_else(|| state.session.lock().unwrap().clone())
        .ok_or("not logged in")?;
    api::get_plans(&url, &tok).await
}

#[tauri::command]
async fn claim_ip(
    state: tauri::State<'_, AppState>,
    panel_url: Option<String>,
    session: Option<String>,
    ip: String,
) -> Result<SimpleResponse, String> {
    let url = panel_url
        .or_else(|| state.panel_url.lock().unwrap().clone())
        .ok_or("panel URL not set")?;
    let tok = session
        .or_else(|| state.session.lock().unwrap().clone())
        .ok_or("not logged in")?;
    api::claim_ip(&url, &tok, &ip).await
}

// Async, not sync: a sync command runs on the core thread and `connect`
// waits on several netsh spawns — sync meant the window froze ("Not
// responding") for the whole Connect press. Locks are taken after the await.
#[tauri::command]
async fn connect(state: tauri::State<'_, AppState>, relay_ip: String) -> Result<String, String> {
    dns::start_dns_proxy(&relay_ip).await?;
    *state.connected.lock().unwrap() = true;
    Ok(relay_ip)
}

#[tauri::command]
async fn disconnect(state: tauri::State<'_, AppState>) -> Result<(), String> {
    dns::stop_dns_proxy_async().await;
    *state.connected.lock().unwrap() = false;
    Ok(())
}

#[tauri::command]
fn get_dns_status() -> Result<DnsStatus, String> {
    dns::get_dns_status()
}

#[tauri::command]
fn check_relay_connection(relay_ip: String) -> Result<bool, String> {
    dns::check_relay_connection(&relay_ip)
}

// spawn_blocking: spawns netstat, must not hold the core thread.
#[tauri::command]
async fn get_net_speed() -> Result<(u64, u64), String> {
    tokio::task::spawn_blocking(dns::get_net_speed)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn resolve_relay_ip(panel_url: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || dns::resolve_relay_ip(&panel_url))
        .await
        .map_err(|e| e.to_string())?
}

// spawn_blocking: opens a socket and waits up to 2s, must not hold the core thread.
#[tauri::command]
async fn resolve_local(domain: String) -> Result<LocalResolve, String> {
    tokio::task::spawn_blocking(move || dns::resolve_local(&domain))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn logout(state: tauri::State<'_, AppState>) -> Result<(), String> {
    *state.session.lock().unwrap() = None;
    Ok(())
}

fn create_tray(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let connect = MenuItem::with_id(app, "connect", "Connect", true, None::<&str>)?;
    let disconnect = MenuItem::with_id(app, "disconnect", "Disconnect", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[
        &connect, &disconnect, &separator, &show, &separator2, &quit,
    ])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("PeDitXCDN")
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "connect" => { let _ = app.emit("tray-connect", ()); }
            "disconnect" => { let _ = app.emit("tray-disconnect", ()); }
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "quit" => {
                // Restore DNS before exiting
                dns::stop_dns_proxy();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

/// True when this process has an elevated admin token.
/// `net session` exits non-zero without one. If the Server service is off
/// this reads false for a real admin — harmless, because ensure_admin's
/// `--elevated` guard bounds it to a single relaunch.
#[cfg(target_os = "windows")]
fn is_elevated() -> bool {
    dns::cmd("net")
        .args(["session"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if running as admin, and relaunch with UAC if not.
/// Uses --elevated flag to prevent infinite restart loop.
#[cfg(target_os = "windows")]
fn ensure_admin() {
    let already_elevated = std::env::args().any(|a| a == "--elevated");

    // Ask about the token, don't infer it from a bind test. Port 53 is not
    // in Windows' excluded ranges by default, so an unelevated process
    // binds it happily, the old check concluded "we have admin", and the
    // first `netsh set dns` then failed with "requires elevation".
    if is_elevated() {
        return;
    }

    // Already relaunched once — a false negative here must not loop.
    if already_elevated {
        eprintln!("[PeDitXCDN] Elevation requested but admin check still fails, continuing...");
        return;
    }

    // Need admin — relaunch with ShellExecuteW "runas"
    let exe_path = std::env::current_exe().expect("failed to get exe path");
    let path_str = exe_path.to_str().unwrap();

    let operation: Vec<u16> = "runas\0".encode_utf16().collect();
    // Append --elevated arg so we don't loop
    let cmd_line = format!("\"{}\" --elevated\0", path_str);
    let cmd: Vec<u16> = cmd_line.encode_utf16().collect();
    let path_w: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            path_w.as_ptr(),
            cmd.as_ptr(),
            std::ptr::null(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
    }
    std::process::exit(0);
}

#[cfg(not(target_os = "windows"))]
fn ensure_admin() {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Ensure running as admin (needed for DNS proxy on port 53)
    ensure_admin();

    // Log panics to file for debugging — writes to %APPDATA%/PeDitXCDN/crash.log
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {}\n", info);
        if let Some(dir) = dirs::data_dir() {
            let log_dir = dir.join("PeDitXCDN");
            let _ = std::fs::create_dir_all(&log_dir);
            let _ = std::fs::write(log_dir.join("crash.log"), &msg);
        }
        // Also try current dir as fallback
        let _ = std::fs::write("PeDitXCDN-crash.log", &msg);
    }));

    // Log startup
    if let Some(dir) = dirs::data_dir() {
        let log_dir = dir.join("PeDitXCDN");
        let _ = std::fs::create_dir_all(&log_dir);
        let _ = std::fs::write(
            log_dir.join("crash.log"),
            format!("PeDitXCDN started at {:?}\n", std::time::SystemTime::now()),
        );
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            connected: Mutex::new(false),
            session: Mutex::new(None),
            panel_url: Mutex::new(None),
        })
        .setup(|app| {
            // Crash recovery: if DNS is stuck at 127.0.0.1 from a previous crash,
            // restore it directly (can't use stop_dns_proxy here — no tokio runtime yet)
            if let Ok(status) = dns::get_dns_status() {
                if status.current_dns.as_deref() == Some("127.0.0.1") {
                    eprintln!("[PeDitXCDN] Found stale proxy DNS, restoring DHCP...");
                    dns::restore_system_dns();
                }
            }

            // Create tray - non-fatal if it fails
            if let Err(e) = create_tray(app.handle()) {
                eprintln!("Warning: tray icon failed: {e}");
            }

            // Hide to tray on close instead of quitting + cleanup
            if let Some(win) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Stop DNS proxy asynchronously (close handler runs on main thread)
                        tauri::async_runtime::spawn(async {
                            dns::stop_dns_proxy_async().await;
                        });
                        // Hide window to system tray
                        if let Some(win) = handle.get_webview_window("main") {
                            let _ = win.hide();
                        }
                        api.prevent_close();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            login,
            signup,
            get_user_info,
            get_plans,
            claim_ip,
            connect,
            disconnect,
            get_dns_status,
            check_relay_connection,
            resolve_relay_ip,
            resolve_local,
            get_net_speed,
            logout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
