//! Tauri command layer: thin wrappers around [`crate::manager::ManagerCore`].

use crate::error::Result;
use crate::manager::ConnectOptions;
use crate::AgentKey;
use agent_key_core::{
    ApprovalRequest, ApprovalResolution, ButtonGesture, CurrentState, DeviceInfo, Health,
    StatusUpdate, SubmitOutcome,
};
use serde::Serialize;
use tauri::{command, State};

/// What `request_approval` returns to the caller: either a pending id or an
/// immediate policy resolution (critical -> denied).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum ApprovalOutcome {
    Pending { id: String },
    Resolved(ApprovalResolution),
}

impl From<SubmitOutcome> for ApprovalOutcome {
    fn from(o: SubmitOutcome) -> Self {
        match o {
            SubmitOutcome::Pending { id } => ApprovalOutcome::Pending { id },
            SubmitOutcome::Resolved(res) => ApprovalOutcome::Resolved(res),
        }
    }
}

#[command]
pub async fn list_devices(agent_key: State<'_, AgentKey>) -> Result<Vec<DeviceInfo>> {
    Ok(agent_key.0.list_devices())
}

#[command]
pub async fn connect(
    agent_key: State<'_, AgentKey>,
    options: Option<ConnectOptions>,
) -> Result<DeviceInfo> {
    agent_key.0.connect(options.unwrap_or(ConnectOptions {
        transport: None,
        port: None,
    }))
}

/// Disconnect one device by id, or every device when `id` is omitted.
#[command]
pub async fn disconnect(agent_key: State<'_, AgentKey>, id: Option<String>) -> Result<()> {
    agent_key.0.disconnect(id.as_deref())
}

#[command]
pub async fn get_health(agent_key: State<'_, AgentKey>) -> Result<Health> {
    Ok(agent_key.0.get_health())
}

#[command]
pub async fn set_status(
    agent_key: State<'_, AgentKey>,
    status: StatusUpdate,
) -> Result<CurrentState> {
    agent_key.0.set_status(status)
}

#[command]
pub async fn request_approval(
    agent_key: State<'_, AgentKey>,
    request: ApprovalRequest,
) -> Result<ApprovalOutcome> {
    agent_key.0.request_approval(request).map(Into::into)
}

#[command]
pub async fn cancel_approval(
    agent_key: State<'_, AgentKey>,
    id: String,
) -> Result<ApprovalResolution> {
    agent_key.0.cancel_approval(&id)
}

#[command]
pub async fn get_current_state(
    agent_key: State<'_, AgentKey>,
) -> Result<CurrentState> {
    Ok(agent_key.0.get_current_state())
}

#[command]
pub async fn set_brightness(
    agent_key: State<'_, AgentKey>,
    value: u8,
) -> Result<CurrentState> {
    agent_key.0.set_brightness(value)
}

/// Dev helper: inject a synthetic button gesture (MockTransport only).
#[command]
pub async fn simulate_button(
    agent_key: State<'_, AgentKey>,
    gesture: ButtonGesture,
) -> Result<()> {
    agent_key.0.simulate_button(gesture)
}
