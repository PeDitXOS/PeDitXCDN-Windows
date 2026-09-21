mod api;
mod dns;
mod types;

use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use types::{DnsStatus, LoginResponse, PlansResponse, SimpleResponse, UserInfo};

/// Tracks connection state and session info.
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

#[tauri::command]
fn connect(state: tauri::State<'_, AppState>, relay_ip: String) -> Result<String, String> {
    dns::set_dns(&relay_ip)?;
    *state.connected.lock().unwrap() = true;
    Ok(relay_ip)
}

#[tauri::command]
fn disconnect(state: tauri::State<'_, AppState>) -> Result<(), String> {
    dns::restore_dns()?;
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
            "quit" => { app.exit(0); }
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState {
            connected: Mutex::new(false),
            session: Mutex::new(None),
            panel_url: Mutex::new(None),
        })
        .setup(|app| {
            create_tray(app.handle())?;
            if let Some(win) = app.get_webview_window("main") {
                win.on_window_event(|event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
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
            logout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
