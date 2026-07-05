//! agent-key tray app (Phase 4).
//!
//! System-tray resident host for the agent-key plugin:
//! - embeds the plugin (localhost API on 127.0.0.1:43117 for the CLI/hooks)
//! - tray menu: connection status, device selection, brightness, autostart
//! - toast notification when a physical approval is requested/resolved
//! - auto-detects the JEWEL1k (serial CDC first, then raw HID) and
//!   re-connects automatically after unplug/replug

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use jewel1k_plugin_agent_key::{AgentKey, ConnectOptions};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Listener, Manager, Wry};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_notification::NotificationExt as _;

/// Toggled from the tray menu; the reconnect thread reads it every cycle.
static AUTO_RECONNECT: AtomicBool = AtomicBool::new(true);

const RECONNECT_INTERVAL: Duration = Duration::from_secs(3);

/// USB identity of the JEWEL1k. The HID composite firmware overrides the
/// VID/PID to this (keyboardConfig.h / agent-key-core / transport/hid.rs).
const JEWEL_VIDPID: &str = "4249:4287";

/// The CDC firmware (src/agentkey/agentkey.ino) ships with "Default CDC"
/// USB settings, so it enumerates under CH55xduino's stock pid.codes ID
/// instead of JEWEL_VIDPID. Match it too, or the serial device shows up as a
/// plain "USB Serial Device" and never gets auto-connected.
const CH55X_CDC_VIDPID: &str = "1209:c550";

fn main() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(jewel1k_plugin_agent_key::init())
        .setup(|app| {
            setup_tray(app.handle())?;
            setup_notifications(app.handle());
            spawn_reconnect_thread(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the status window hides it; the app lives in the tray.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running agent-key tray app");
}

fn core(app: &AppHandle<Wry>) -> std::sync::Arc<jewel1k_plugin_agent_key::ManagerCore> {
    app.state::<AgentKey>().0.clone()
}

fn setup_tray(app: &AppHandle<Wry>) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "未接続", false, None::<&str>)?;
    let auto = CheckMenuItem::with_id(
        app,
        "auto-reconnect",
        "自動接続 (シリアル/HID)",
        true,
        true,
        None::<&str>,
    )?;
    let connect_mock = MenuItem::with_id(
        app,
        "connect-mock",
        "Mock デバイスに接続 (実機なし)",
        true,
        None::<&str>,
    )?;
    let disconnect = MenuItem::with_id(app, "disconnect", "切断", true, None::<&str>)?;
    let brightness = Submenu::with_id_and_items(
        app,
        "brightness",
        "輝度",
        true,
        &[
            &MenuItem::with_id(app, "brightness-64", "25%", true, None::<&str>)?,
            &MenuItem::with_id(app, "brightness-128", "50%", true, None::<&str>)?,
            &MenuItem::with_id(app, "brightness-255", "100%", true, None::<&str>)?,
        ],
    )?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "ログイン時に起動",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let show = MenuItem::with_id(app, "show", "ステータスウィンドウ", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &auto,
            &connect_mock,
            &disconnect,
            &brightness,
            &PredefinedMenuItem::separator(app)?,
            &show,
            &autostart,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))?;
    let tray = TrayIconBuilder::with_id("agent-key-tray")
        .icon(icon)
        .tooltip("JEWEL1k agent-key")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            let core = core(app);
            match event.id().as_ref() {
                "auto-reconnect" => {
                    let enabled = !AUTO_RECONNECT.load(Ordering::Relaxed);
                    AUTO_RECONNECT.store(enabled, Ordering::Relaxed);
                }
                "connect-mock" => {
                    if let Err(e) = core.connect(ConnectOptions {
                        transport: Some("mock".into()),
                        port: None,
                    }) {
                        log::error!("mock connect failed: {e}");
                    }
                }
                "disconnect" => {
                    // Manual disconnect also implies "stop reconnecting".
                    AUTO_RECONNECT.store(false, Ordering::Relaxed);
                    let _ = core.disconnect(None);
                }
                "brightness-64" => {
                    let _ = core.set_brightness(64);
                }
                "brightness-128" => {
                    let _ = core.set_brightness(128);
                }
                "brightness-255" => {
                    let _ = core.set_brightness(255);
                }
                "autostart" => {
                    let autolaunch = app.autolaunch();
                    let enabled = autolaunch.is_enabled().unwrap_or(false);
                    let result = if enabled {
                        autolaunch.disable()
                    } else {
                        autolaunch.enable()
                    };
                    if let Err(e) = result {
                        log::error!("autostart toggle failed: {e}");
                    }
                }
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .build(app)?;

    // Tauri keeps registered tray icons alive; the handle itself is not
    // needed past this point.
    let _ = tray;

    // Keep the status line and tooltip in sync with device events.
    let app_ = app.clone();
    let update = move |text: String| {
        let _ = status.set_text(&text);
        if let Some(tray) = app_.tray_by_id("agent-key-tray") {
            let _ = tray.set_tooltip(Some(format!("JEWEL1k agent-key — {text}")));
        }
    };

    {
        let update = update.clone();
        app.listen_any("agent-key://device-connected", move |event| {
            let device: Value = serde_json::from_str(event.payload()).unwrap_or(Value::Null);
            let name = device
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("device");
            update(format!("接続中: {name}"));
        });
    }
    {
        let update = update.clone();
        app.listen_any("agent-key://device-disconnected", move |_| {
            update("未接続".to_string());
        });
    }
    Ok(())
}

fn setup_notifications(app: &AppHandle<Wry>) {
    {
        let app_ = app.clone();
        app.listen_any("agent-key://approval-requested", move |event| {
            let req: Value = serde_json::from_str(event.payload()).unwrap_or(Value::Null);
            let title = req.get("title").and_then(Value::as_str).unwrap_or("action");
            let risk = req.get("risk").and_then(Value::as_str).unwrap_or("medium");
            let body = format!(
                "{title}\nリスク: {risk} — キー単押し=承認 / 長押し=拒否 / ダブル=詳細"
            );
            if let Err(e) = app_
                .notification()
                .builder()
                .title("JEWEL1k 承認要求")
                .body(body)
                .show()
            {
                log::warn!("notification failed: {e}");
            }
        });
    }
    {
        let app_ = app.clone();
        app.listen_any("agent-key://approval-resolved", move |event| {
            let res: Value = serde_json::from_str(event.payload()).unwrap_or(Value::Null);
            let decision = res
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or("resolved");
            let label = match decision {
                "approved" => "承認されました",
                "denied" => "拒否されました",
                "timed_out" => "タイムアウトしました",
                "cancelled" => "取り消されました",
                "emergency_stopped" => "緊急停止されました",
                other => other,
            };
            let _ = app_
                .notification()
                .builder()
                .title("JEWEL1k 承認結果")
                .body(label)
                .show();
        });
    }
    {
        // Double-press while an approval is pending = "show details".
        let app_ = app.clone();
        app.listen_any("agent-key://button", move |event| {
            let payload: Value = serde_json::from_str(event.payload()).unwrap_or(Value::Null);
            if payload.get("gesture").and_then(Value::as_str) == Some("double") {
                if let Some(window) = app_.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        });
    }
}

/// True when a listed device looks like a JEWEL1k.
fn looks_like_jewel(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains(JEWEL_VIDPID)
        || lower.contains(CH55X_CDC_VIDPID)
        || lower.contains("ch55")
        || lower.contains("jewel")
}

/// Poll for the device while nothing is connected. Serial (CDC firmware)
/// wins over raw HID (composite firmware); mock is never auto-connected.
fn spawn_reconnect_thread(app: AppHandle<Wry>) {
    std::thread::Builder::new()
        .name("agent-key-reconnect".into())
        .spawn(move || loop {
            if AUTO_RECONNECT.load(Ordering::Relaxed) {
                let core = core(&app);
                if !core.get_health().connected {
                    let devices = core.list_devices();
                    let target = devices
                        .iter()
                        .find(|d| d.transport == "serial" && looks_like_jewel(&d.name))
                        .or_else(|| devices.iter().find(|d| d.transport == "hid"));
                    if let Some(device) = target {
                        match core.connect(ConnectOptions {
                            transport: Some(device.transport.clone()),
                            port: device.port.clone(),
                        }) {
                            Ok(d) => log::info!("auto-connected {}", d.id),
                            Err(e) => log::debug!("auto-connect {} failed: {e}", device.id),
                        }
                    }
                }
            }
            std::thread::sleep(RECONNECT_INTERVAL);
        })
        .expect("spawn reconnect thread");
}
