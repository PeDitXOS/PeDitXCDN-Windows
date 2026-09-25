mod api;
mod dns;
mod types;
mod singconf;
mod tunnel;
mod wireproxy;

use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use types::{
    DnsStatus, EmergencyStop, LocalResolve, LoginResponse, PlansResponse, ProxyStatus,
    SimpleResponse, UserInfo,
};

struct AppState {
    connected: Mutex<bool>,
    // Off unless the customer ticks the dashboard box. An always-on claim
    // would evict the account's address from any other device they use
    // (max_ips=1), so opting out has to mean this machine never claims it.
    auto_register: Mutex<bool>,
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
async fn connect(app: tauri::AppHandle, relay_ip: String) -> Result<String, String> {
    do_connect(app, relay_ip).await
}

/// Keep the relay's idea of this account's address current, while connected.
///
/// Every 15 s: mint a nonce, drop it in a DNS query at the relay on
/// UDP/5354, and the relay registers whatever address that query actually
/// left from - so a customer who changes IP twenty times a day is let back
/// in within seconds each time, with no manual registration and no memory
/// of what the address used to be. Stops when Disconnect flips `connected`,
/// and stays idle while the dashboard's auto-register tick is off - that tick
/// is what lets a customer hand the account to another device.
async fn nonce_loop(
    app: tauri::AppHandle,
    relay_ip: String,
    session: String,
    panel_url: String,
) {
    let mut fails: u32 = 0;
    loop {
        let st = app.state::<AppState>();
        let still = *st.connected.lock().unwrap();
        if !still {
            break;
        }
        if !*st.auto_register.lock().unwrap() {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            continue;
        }
        match api::mint_nonce(&panel_url, &session).await {
            Ok(nonce) => {
                fails = 0;
                if let Err(e) = send_nonce(&relay_ip, &nonce) {
                    api::log_to_file(&format!("NONCE udp to {relay_ip}: {e}"));
                }
            }
            Err(e) => {
                fails += 1;
                // One line while it may be a blip, one line an hour once it
                // is not - this runs every 15 s for the whole session.
                if fails == 1 || fails % 240 == 0 {
                    api::log_to_file(&format!("NONCE mint failed ({fails}): {e}"));
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
    }
}

/// Fire-and-forget wire format: a DNS query whose first label is "n" plus
/// the 32-hex nonce. No reply is waited for - the next tick re-asserts, and
/// /user-info shows the registered address whenever the user looks.
fn send_nonce(relay_ip: &str, nonce: &str) -> std::io::Result<()> {
    use std::net::UdpSocket;
    let label = format!("n{nonce}");
    if label.len() > 63 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "nonce too long",
        ));
    }
    let mut pkt = vec![0u8; 12];
    pkt[2] = 0x01; // recursion desired
    pkt[5] = 0x01; // one question
    pkt.push(label.len() as u8);
    pkt.extend_from_slice(label.as_bytes());
    pkt.push(0);
    pkt.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]); // A, IN
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.send_to(&pkt, (relay_ip, 5354))?;
    Ok(())
}

/// Dashboard tick: claim this machine's address automatically or not. Pushed
/// from the webview on mount and on every toggle; the mount-time claim and
/// `nonce_loop` both read it.
#[tauri::command]
fn set_auto_register(state: tauri::State<'_, AppState>, on: bool) {
    *state.auto_register.lock().unwrap() = on;
}

#[tauri::command]
async fn disconnect(app: tauri::AppHandle) -> Result<(), String> {
    do_disconnect(app).await
}

/// Real service state — a Mutex read, so the dashboard can poll it every
/// second and stop trusting its own `connected` flag (which kept saying
/// «متصل» after a tray cut or a dead loop).
#[tauri::command]
fn proxy_status() -> ProxyStatus {
    dns::proxy_status()
}

/// One-key emergency cut: stop the loops and force DHCP back on both
/// stacks, then report what the system actually ended up with.
/// spawn_blocking — the restore is a multi-interface netsh loop plus
/// flushdns and must not sit on the core thread (the v0.3.15 lesson).
#[tauri::command]
async fn emergency_stop(state: tauri::State<'_, AppState>) -> Result<EmergencyStop, String> {
    // Before the stop: `connected` is what keeps nonce_loop alive.
    *state.connected.lock().unwrap() = false;
    // dns::emergency_stop returns the report itself, not a Result — the `?`
    // only unwraps the JoinError, so the tail has to be wrapped back in Ok.
    Ok(tokio::task::spawn_blocking(dns::emergency_stop)
        .await
        .map_err(|e| e.to_string())?)
}

// async + spawn_blocking: get_dns_status runs netsh twice (v4 and v6) and
// check_relay_connection runs a ping with a 1s wait — both were sync commands,
// i.e. sitting on the core thread whenever the webview asked for status.
#[tauri::command]
async fn get_dns_status() -> Result<DnsStatus, String> {
    tokio::task::spawn_blocking(dns::get_dns_status)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn check_relay_connection(relay_ip: String) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || dns::check_relay_connection(&relay_ip))
        .await
        .map_err(|e| e.to_string())?
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

// spawn_blocking: opens a socket and waits up to 2s (twice on failure —
// the relay hop is probed too), must not hold the core thread.
#[tauri::command]
async fn resolve_local(domain: String, relay_ip: Option<String>) -> Result<LocalResolve, String> {
    tokio::task::spawn_blocking(move || dns::resolve_local(&domain, relay_ip.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn logout(state: tauri::State<'_, AppState>) -> Result<(), String> {
    *state.session.lock().unwrap() = None;
    Ok(())
}

/// Bring the window back from the tray (menu item and icon click both).
// ─── sing-box tunnel ──────────────────────────────────────────────────────
//
// Every one of these is async and every blocking call goes through
// `spawn_blocking`: v0.3.16 shipped a frozen window because netsh ran on the
// core thread, and reading a config off disk is the same mistake smaller.

#[tauri::command]
async fn tunnel_status() -> Result<tunnel::TunnelStatus, String> {
    tokio::task::spawn_blocking(tunnel::status)
        .await
        .map_err(|e| e.to_string())
}

/// Store and validate a pasted body. Does not start anything — a config the
/// parser rejects leaves the previous one in place.
#[tauri::command]
async fn tunnel_set_config(body: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tunnel::set_config(&body))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tunnel_add_app(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tunnel::add_app(path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tunnel_remove_app(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tunnel::remove_app(path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tunnel_start(relay_ip: String, panel_ip: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tunnel::start(relay_ip, panel_ip))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn tunnel_stop() -> Result<(), String> {
    tokio::task::spawn_blocking(|| tunnel::stop("user"))
        .await
        .map_err(|e| e.to_string())?
}

/// Live proof of what is tunneled, straight from clash_api.
#[tauri::command]
async fn tunnel_connections() -> Result<Vec<tunnel::TunnelConn>, String> {
    tunnel::connections().await
}

/// The settings the page shows, from disk if this is the first read.
#[tauri::command]
async fn tunnel_opts_get() -> Result<singconf::Opts, String> {
    tokio::task::spawn_blocking(tunnel::opts)
        .await
        .map_err(|e| e.to_string())
}

/// Save, regenerate, and restart a running tunnel so the change is real.
#[tauri::command]
async fn tunnel_opts_set(opts: singconf::Opts) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tunnel::set_opts(opts))
        .await
        .map_err(|e| e.to_string())?
}

/// The config sing-box would be handed right now — what was written, not
/// what the UI thinks was written.
#[tauri::command]
async fn tunnel_preview() -> Result<String, String> {
    tokio::task::spawn_blocking(tunnel::preview)
        .await
        .map_err(|e| e.to_string())?
}

/// Send one ticked app to one named config. Empty tag = back to the default.
#[tauri::command]
async fn tunnel_set_route(path: String, tag: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tunnel::set_route(path, tag))
        .await
        .map_err(|e| e.to_string())?
}

/// The switch beside a config in the list.
#[tauri::command]
async fn tunnel_set_cfg(tag: String, on: bool) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tunnel::set_cfg(tag, on))
        .await
        .map_err(|e| e.to_string())?
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// Shared by the `connect` command and the tray's «اتصال» — the tray path
/// has to start nonce_loop too, or auto-registration silently dies on a
/// reconnect that didn't come from the button. Takes the AppHandle, not a
/// `&AppState`: the tray has no `tauri::State`, and the state access here
/// lives in a scope that ends before any await, so no borrow of the managed
/// state is ever carried across one.
async fn do_connect(app: tauri::AppHandle, relay_ip: String) -> Result<String, String> {
    dns::start_dns_proxy(&relay_ip).await?;
    // The clones must be bound inside the block: as a block-tail tuple the
    // MutexGuard temporaries outlive `st` (E0597 — `st` does not live long enough).
    let (session, panel_url) = {
        let st = app.state::<AppState>();
        *st.connected.lock().unwrap() = true;
        let session = st.session.lock().unwrap().clone();
        let panel_url = st.panel_url.lock().unwrap().clone();
        (session, panel_url)
    };
    if let (Some(session), Some(panel_url)) = (session, panel_url) {
        tauri::async_runtime::spawn(nonce_loop(app, relay_ip.clone(), session, panel_url));
    }
    Ok(relay_ip)
}

async fn do_disconnect(app: tauri::AppHandle) -> Result<(), String> {
    dns::stop_dns_proxy_async().await;
    *app.state::<AppState>().connected.lock().unwrap() = false;
    Ok(())
}

fn create_tray(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let connect = MenuItem::with_id(app, "connect", "اتصال", true, None::<&str>)?;
    let disconnect = MenuItem::with_id(app, "disconnect", "قطع اتصال", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let emergency = MenuItem::with_id(app, "emergency", "قطع اضطراری", true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let show = MenuItem::with_id(app, "show", "نمایش", true, None::<&str>)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "خروج", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[
        &connect, &disconnect, &sep1, &emergency, &sep2, &show, &sep3, &quit,
    ])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("PeDitXCDN")
        // Every item runs here, in the backend, rather than emitting an
        // event for the webview to answer: the old `tray-connect` /
        // `tray-disconnect` pair had no listener (a menu that does
        // nothing), and the emergency cut has to work with a hung webview.
        // The dashboard's 1 s `proxy_status` poll is what reflects all of
        // this back into the UI.
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "connect" => {
                let relay = dns::last_relay();
                if let Some(relay) = relay {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = do_connect(app, relay).await;
                    });
                } else {
                    // Nothing to reconnect to in this session — show the
                    // window so the user can press Connect there instead of
                    // clicking a menu item that silently did nothing.
                    show_main(app);
                }
            }
            "disconnect" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = do_disconnect(app).await;
                });
            }
            "emergency" => {
                // On its own thread: the restore is seconds of netsh and
                // must not wedge the menu, and `connected` flips first so
                // nonce_loop stops talking to the relay.
                let app = app.clone();
                std::thread::spawn(move || {
                    *app.state::<AppState>().connected.lock().unwrap() = false;
                    dns::emergency_stop();
                });
            }
            "show" => show_main(app),
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
                show_main(tray.app_handle());
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
            auto_register: Mutex::new(false),
            session: Mutex::new(None),
            panel_url: Mutex::new(None),
        })
        .setup(|app| {
            // Crash recovery: if DNS is stuck on the proxy from a previous
            // crash (or a killed "Not responding" window), restore it.
            // Off the main thread — the restore is a multi-interface netsh
            // loop plus flushdns, and running it inline held setup() (and so
            // the first paint) for seconds on exactly the machines that need
            // it. Both stacks: a leftover ::1 breaks resolution as hard as a
            // leftover 127.0.0.1, and get_dns_status used to report neither.
            std::thread::spawn(|| {
                if let Ok(status) = dns::get_dns_status() {
                    // Same predicate as the emergency cut — they cannot disagree.
                    if dns::points_at_proxy(&status) {
                        eprintln!("[PeDitXCDN] Found stale proxy DNS, restoring DHCP...");
                        api::log_to_file("STALE DNS found at startup, restoring DHCP");
                        let _ = dns::restore_system_dns();
                        api::log_to_file("STALE DNS restored");
                    }
                }
            });

            // Create tray - non-fatal if it fails
            if let Err(e) = create_tray(app.handle()) {
                eprintln!("Warning: tray icon failed: {e}");
            }

            // Hide to tray on close instead of quitting + cleanup
            if let Some(win) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Hide only — the connection keeps running. Stopping the
                        // proxy here meant closing the window silently dropped
                        // the tunnel: DNS went back to the ISP (YouTube dead)
                        // while the still-alive webview kept showing «متصل» and
                        // the probe timed out. Stop happens on Disconnect,
                        // Logout and tray Quit, which are the explicit acts.
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
            set_auto_register,
            connect,
            disconnect,
            proxy_status,
            emergency_stop,
            get_dns_status,
            check_relay_connection,
            resolve_relay_ip,
            resolve_local,
            get_net_speed,
            logout,
            tunnel_status,
            tunnel_set_config,
            tunnel_add_app,
            tunnel_remove_app,
            tunnel_start,
            tunnel_stop,
            tunnel_connections,
            tunnel_opts_get,
            tunnel_opts_set,
            tunnel_preview,
            tunnel_set_route,
            tunnel_set_cfg,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
