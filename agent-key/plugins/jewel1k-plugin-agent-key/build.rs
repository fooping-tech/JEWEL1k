const COMMANDS: &[&str] = &[
    "list_devices",
    "connect",
    "disconnect",
    "get_health",
    "set_status",
    "request_approval",
    "cancel_approval",
    "get_current_state",
    "set_brightness",
    "simulate_button",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
