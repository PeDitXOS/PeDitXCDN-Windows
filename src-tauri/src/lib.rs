mod dns;
mod panel;
mod types;

use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use types::{DnsStatus, PanelData};

/// Default relay IP — change this to match your deployment.
const RELAY_IP: &str = "92.42.207.101";

/// Tracks whether we're currently using relay DNS.
struct ConnectionState {
    connected: Mutex<bool>,
}

#[tauri::command]
fn connect(state: tauri::State<'_, ConnectionState>) -> Result<String, String> {
    dns::set_dns(RELAY_IP)?;
    *state.connected.lock().unwrap() = true;
    Ok(RELAY_IP.to_string())
}

#[tauri::command]
fn disconnect(state: tauri::State<'_, ConnectionState>) -> Result<(), String> {
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
async fn get_panel_data() -> Result<PanelData, String> {
    panel::fetch_panel_data().await
}

#[tauri::command]
fn open_panel() -> Result<(), String> {
    tauri_plugin_opener::open_url("https://docproir.peditxcdn.ir:8443", None::<&str>)
        .map_err(|e| e.to_string())
}

fn create_tray(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let connect = MenuItem::with_id(app, "connect", "Connect", true, None::<&str>)?;
    let disconnect = MenuItem::with_id(app, "disconnect", "Disconnect", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let show = MenuItem::with_id(app, "show", "Show Panel", true, None::<&str>)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[
        &connect,
        &disconnect,
        &separator,
        &show,
        &separator2,
        &quit,
    ])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("PeDitXCDN")
        .on_menu_event(move |app, event| {
            match event.id().as_ref() {
                "connect" => {
                    let _ = app.emit("tray-connect", ());
                }
                "disconnect" => {
                    let _ = app.emit("tray-disconnect", ());
                }
                "show" => {
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
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
        .manage(ConnectionState {
            connected: Mutex::new(false),
        })
        .setup(|app| {
            create_tray(app.handle())?;

            // Hide to tray on close instead of quitting
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
            connect,
            disconnect,
            get_dns_status,
            check_relay_connection,
            get_panel_data,
            open_panel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
