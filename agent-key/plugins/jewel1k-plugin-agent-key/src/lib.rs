//! jewel1k-plugin-agent-key
//!
//! Tauri v2 plugin that turns a JEWEL1k 1-key keyboard into a status LED and
//! physical approval button for AI coding agents.
//!
//! ```rust,ignore
//! fn main() {
//!     tauri::Builder::default()
//!         .plugin(jewel1k_plugin_agent_key::init())
//!         .run(tauri::generate_context!())
//!         .expect("error while running tauri application");
//! }
//! ```
//!
//! Configuration (tauri.conf.json):
//! ```json
//! {
//!   "plugins": {
//!     "agent-key": {
//!       "httpEnabled": true,
//!       "httpPort": 43117,
//!       "httpToken": null,
//!       "autoConnect": "mock"
//!     }
//!   }
//! }
//! ```

mod commands;
mod error;
mod manager;
mod server;
#[cfg(test)]
mod tests;

pub use error::{Error, Result};
pub use manager::{ConnectOptions, ManagerCore};
pub use server::DEFAULT_PORT;

use serde::Deserialize;
use std::sync::Arc;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Emitter as _, Manager as _, Runtime};

/// Managed state wrapper so apps can also reach the manager from Rust:
/// `app.state::<AgentKey>().0.set_status(...)`.
pub struct AgentKey(pub Arc<ManagerCore>);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// Serve the localhost API for CLI / hooks (127.0.0.1 only).
    pub http_enabled: bool,
    /// Port for the localhost API. 0 picks an ephemeral port.
    pub http_port: u16,
    /// Optional shared secret; clients send it as `x-agent-key-token`.
    pub http_token: Option<String>,
    /// Transport to connect at startup: "mock", "serial:<PORT>" or "none".
    pub auto_connect: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            http_enabled: true,
            http_port: server::DEFAULT_PORT,
            http_token: None,
            auto_connect: "mock".into(),
        }
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R, Option<Config>> {
    Builder::<R, Option<Config>>::new("agent-key")
        .invoke_handler(tauri::generate_handler![
            commands::list_devices,
            commands::connect,
            commands::disconnect,
            commands::get_health,
            commands::set_status,
            commands::request_approval,
            commands::cancel_approval,
            commands::get_current_state,
            commands::set_brightness,
            commands::simulate_button,
        ])
        .setup(|app, api| {
            let config = api.config().clone().unwrap_or_default();
            let emit_handle = app.clone();
            let core = Arc::new(ManagerCore::new(Box::new(move |event, payload| {
                if let Err(e) = emit_handle.emit(event, payload) {
                    log::warn!("failed to emit {event}: {e}");
                }
            })));
            app.manage(AgentKey(core.clone()));

            match config.auto_connect.as_str() {
                "none" | "" => {}
                "mock" => {
                    if let Err(e) = core.connect(ConnectOptions {
                        transport: Some("mock".into()),
                        port: None,
                    }) {
                        log::warn!("auto-connect mock failed: {e}");
                    }
                }
                other => {
                    if let Some(port) = other.strip_prefix("serial:") {
                        if let Err(e) = core.connect(ConnectOptions {
                            transport: Some("serial".into()),
                            port: Some(port.to_string()),
                        }) {
                            log::warn!("auto-connect {other} failed: {e}");
                        }
                    } else {
                        log::warn!("unknown autoConnect value: {other}");
                    }
                }
            }

            // Transport poll loop: button events, approval timeouts.
            {
                let core = core.clone();
                std::thread::Builder::new()
                    .name("agent-key-poll".into())
                    .spawn(move || loop {
                        core.tick();
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    })?;
            }

            if config.http_enabled {
                let server_config = server::ServerConfig {
                    port: config.http_port,
                    token: config.http_token.clone(),
                };
                if let Err(e) = server::spawn(core, server_config) {
                    log::error!("failed to start agent-key localhost API: {e}");
                }
            }
            Ok(())
        })
        .build()
}
