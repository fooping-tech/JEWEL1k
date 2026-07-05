//! Plugin state management. One `ManagerCore` per app, shared (via `Arc`)
//! between Tauri commands, the transport poll thread and the localhost API.
//!
//! Safety invariant: approval decisions are produced only by
//! [`agent_key_core::ApprovalQueue`] from device button gestures (or by
//! timeout/cancel/policy). No public method resolves a request as approved.

use crate::error::{Error, Result};
use agent_key_core::{
    led_policy, transport, ApprovalQueue, ApprovalRequest, ApprovalResolution, ButtonGesture,
    CurrentState, DeviceEvent, DeviceInfo, Health, MockTransport, QueueEvent, StatusUpdate,
    SubmitOutcome, Transport,
};
use agent_key_core::types::{AgentState, RiskLevel};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Callback used to broadcast plugin events (wired to `AppHandle::emit`).
/// Injected so the manager stays independent of the Tauri runtime generics.
pub type EmitFn = Box<dyn Fn(&str, serde_json::Value) + Send + Sync>;

pub const EVT_BUTTON: &str = "agent-key://button";
pub const EVT_STATE_CHANGED: &str = "agent-key://state-changed";
pub const EVT_APPROVAL_REQUESTED: &str = "agent-key://approval-requested";
pub const EVT_APPROVAL_PROGRESS: &str = "agent-key://approval-progress";
pub const EVT_APPROVAL_RESOLVED: &str = "agent-key://approval-resolved";
pub const EVT_DEVICE_CONNECTED: &str = "agent-key://device-connected";
pub const EVT_DEVICE_DISCONNECTED: &str = "agent-key://device-disconnected";
pub const EVT_ERROR: &str = "agent-key://error";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectOptions {
    /// "mock" or "serial".
    #[serde(default)]
    pub transport: Option<String>,
    /// Serial port name (e.g. "COM5", "/dev/tty.usbmodemXXXX").
    #[serde(default)]
    pub port: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ButtonEventPayload {
    pub gesture: ButtonGesture,
    pub timestamp_ms: u64,
}

struct Inner {
    transport: Option<Box<dyn Transport>>,
    device: Option<DeviceInfo>,
    state: AgentState,
    risk: RiskLevel,
    brightness: u8,
    /// State to restore once the approval queue drains.
    resume_state: (AgentState, RiskLevel),
    queue: ApprovalQueue,
    resolutions: HashMap<String, ApprovalResolution>,
    resolution_order: Vec<String>,
    last_io_ms: Option<u64>,
}

pub struct ManagerCore {
    emit_fn: EmitFn,
    inner: Mutex<Inner>,
    origin: Instant,
}

impl ManagerCore {
    pub fn new(emit_fn: EmitFn) -> Self {
        Self {
            emit_fn,
            inner: Mutex::new(Inner {
                transport: None,
                device: None,
                state: AgentState::Idle,
                risk: RiskLevel::None,
                brightness: 255,
                resume_state: (AgentState::Idle, RiskLevel::None),
                queue: ApprovalQueue::new(),
                resolutions: HashMap::new(),
                resolution_order: Vec::new(),
                last_io_ms: None,
            }),
            origin: Instant::now(),
        }
    }

    pub fn now_ms(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }

    fn emit<S: Serialize>(&self, event: &str, payload: S) {
        match serde_json::to_value(payload) {
            Ok(value) => (self.emit_fn)(event, value),
            Err(e) => log::warn!("failed to serialize payload for {event}: {e}"),
        }
    }

    // ---- devices -----------------------------------------------------

    pub fn list_devices(&self) -> Vec<DeviceInfo> {
        transport::list_devices()
    }

    pub fn connect(&self, options: ConnectOptions) -> Result<DeviceInfo> {
        let kind = options.transport.as_deref().unwrap_or("mock");
        let (transport, device): (Box<dyn Transport>, DeviceInfo) = match kind {
            "mock" => (
                Box::new(MockTransport::new()),
                DeviceInfo {
                    id: "mock".into(),
                    name: "Mock JEWEL1k (no hardware)".into(),
                    transport: "mock".into(),
                    port: None,
                },
            ),
            #[cfg(feature = "serial")]
            "serial" => {
                let port = options
                    .port
                    .clone()
                    .ok_or_else(|| Error::InvalidInput("serial transport needs `port`".into()))?;
                let t = agent_key_core::transport::SerialTransport::open(&port)?;
                (
                    Box::new(t),
                    DeviceInfo {
                        id: port.clone(),
                        name: format!("JEWEL1k on {port}"),
                        transport: "serial".into(),
                        port: Some(port),
                    },
                )
            }
            other => {
                return Err(Error::UnknownDevice(other.to_string()));
            }
        };

        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(mut old) = inner.transport.take() {
                old.close();
            }
            inner.transport = Some(transport);
            inner.device = Some(device.clone());
        }
        self.emit(EVT_DEVICE_CONNECTED, device.clone());
        self.push_led();
        Ok(device)
    }

    pub fn disconnect(&self) -> Result<()> {
        let device = {
            let mut inner = self.inner.lock().unwrap();
            if let Some(mut t) = inner.transport.take() {
                t.close();
            }
            inner.device.take()
        };
        if device.is_some() {
            self.emit(EVT_DEVICE_DISCONNECTED, serde_json::json!({}));
        }
        Ok(())
    }

    pub fn get_health(&self) -> Health {
        let inner = self.inner.lock().unwrap();
        Health {
            connected: inner
                .transport
                .as_ref()
                .map(|t| t.is_connected())
                .unwrap_or(false),
            device: inner.device.clone(),
            last_io_ms: inner.last_io_ms.map(|t| self.now_ms().saturating_sub(t)),
        }
    }

    // ---- status / LED ------------------------------------------------

    pub fn set_status(&self, update: StatusUpdate) -> Result<CurrentState> {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.state = update.state;
            if let Some(risk) = update.risk {
                inner.risk = risk;
            }
            // Approvals temporarily override the LED; remember what the
            // agent wants shown afterwards.
            inner.resume_state = (inner.state, inner.risk);
            if !inner.queue.is_empty() {
                // keep showing needs_approval while requests are pending
                inner.state = AgentState::NeedsApproval;
            }
        }
        self.push_led();
        let snapshot = self.get_current_state();
        self.emit(EVT_STATE_CHANGED, snapshot.clone());
        Ok(snapshot)
    }

    pub fn set_brightness(&self, value: u8) -> Result<CurrentState> {
        self.inner.lock().unwrap().brightness = value;
        self.push_led();
        let snapshot = self.get_current_state();
        self.emit(EVT_STATE_CHANGED, snapshot.clone());
        Ok(snapshot)
    }

    pub fn get_current_state(&self) -> CurrentState {
        let inner = self.inner.lock().unwrap();
        CurrentState {
            state: inner.state,
            risk: inner.risk,
            brightness: inner.brightness,
            connected: inner
                .transport
                .as_ref()
                .map(|t| t.is_connected())
                .unwrap_or(false),
            pending_approval: inner.queue.current().cloned(),
        }
    }

    /// Send the LED packet for the current state. Errors are reported via
    /// the error event; a missing transport is fine (dev without device).
    fn push_led(&self) {
        let mut inner = self.inner.lock().unwrap();
        let packet = led_policy::packet_for(inner.state, inner.risk, inner.brightness);
        let now = self.now_ms();
        let mut failure = None;
        if let Some(t) = inner.transport.as_mut() {
            match t.send_packet(&packet) {
                Ok(()) => inner.last_io_ms = Some(now),
                Err(e) => failure = Some(e.to_string()),
            }
        } else {
            log::debug!("no transport connected; LED packet skipped: {packet:?}");
        }
        drop(inner);
        if let Some(msg) = failure {
            self.handle_transport_failure(&msg);
        }
    }

    fn handle_transport_failure(&self, msg: &str) {
        log::error!("transport failure: {msg}");
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(mut t) = inner.transport.take() {
                t.close();
            }
            inner.device = None;
        }
        self.emit(EVT_ERROR, serde_json::json!({ "message": msg }));
        self.emit(EVT_DEVICE_DISCONNECTED, serde_json::json!({}));
    }

    // ---- approvals -----------------------------------------------------

    /// Submit an approval request. Returns the resolution immediately when
    /// policy resolves it (critical -> denied), otherwise the pending id.
    pub fn request_approval(&self, request: ApprovalRequest) -> Result<SubmitOutcome> {
        let now = self.now_ms();
        let outcome = {
            let mut inner = self.inner.lock().unwrap();
            let outcome = inner.queue.submit(request.clone(), now);
            if matches!(outcome, SubmitOutcome::Pending { .. }) {
                inner.state = AgentState::NeedsApproval;
                inner.risk = inner.queue.current().map(|r| r.risk).unwrap_or(inner.risk);
            }
            outcome
        };
        match &outcome {
            SubmitOutcome::Pending { id } => {
                let mut req = request;
                req.id = id.clone();
                self.emit(EVT_APPROVAL_REQUESTED, req);
                self.push_led();
                self.emit(EVT_STATE_CHANGED, self.get_current_state());
            }
            SubmitOutcome::Resolved(res) => {
                let mut req = request;
                req.id = res.id.clone();
                self.emit(EVT_APPROVAL_REQUESTED, req);
                self.record_resolution(res.clone());
            }
        }
        Ok(outcome)
    }

    pub fn cancel_approval(&self, id: &str) -> Result<ApprovalResolution> {
        let event = self
            .inner
            .lock()
            .unwrap()
            .queue
            .cancel(id)
            .ok_or_else(|| Error::ApprovalNotFound(id.to_string()))?;
        let QueueEvent::Resolved(res) = event else {
            unreachable!("cancel always resolves");
        };
        self.record_resolution(res.clone());
        self.after_queue_change();
        Ok(res)
    }

    fn record_resolution(&self, res: ApprovalResolution) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.resolution_order.push(res.id.clone());
            inner.resolutions.insert(res.id.clone(), res.clone());
            // keep the map bounded
            while inner.resolution_order.len() > 200 {
                let old = inner.resolution_order.remove(0);
                inner.resolutions.remove(&old);
            }
        }
        self.emit(EVT_APPROVAL_RESOLVED, res);
    }

    /// Look up a past resolution (used by the localhost API long-poll).
    pub fn get_resolution(&self, id: &str) -> Option<ApprovalResolution> {
        self.inner.lock().unwrap().resolutions.get(id).cloned()
    }

    /// Block until the request resolves or `deadline_ms` passes.
    pub fn wait_for_resolution(&self, id: &str, deadline_ms: u64) -> Option<ApprovalResolution> {
        let start = self.now_ms();
        loop {
            if let Some(res) = self.get_resolution(id) {
                return Some(res);
            }
            if self.now_ms().saturating_sub(start) >= deadline_ms {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// After the queue shrank: restore the agent's own state when empty, or
    /// re-point the LED at the next pending request.
    fn after_queue_change(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.queue.is_empty() {
                let (s, r) = inner.resume_state;
                inner.state = s;
                inner.risk = r;
            } else {
                inner.state = AgentState::NeedsApproval;
                inner.risk = inner
                    .queue
                    .current()
                    .map(|req| req.risk)
                    .unwrap_or(inner.risk);
            }
        }
        self.push_led();
        self.emit(EVT_STATE_CHANGED, self.get_current_state());
    }

    /// Last LED packet written to the transport (mock introspection).
    pub fn last_sent_packet(&self) -> Option<agent_key_core::HostPacket> {
        self.inner
            .lock()
            .unwrap()
            .transport
            .as_ref()
            .and_then(|t| t.last_packet())
    }

    // ---- device events / poll loop -------------------------------------

    pub fn simulate_button(&self, gesture: ButtonGesture) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let t = inner.transport.as_mut().ok_or(Error::NotConnected)?;
        t.inject_event(DeviceEvent::Button { gesture })?;
        Ok(())
    }

    /// One iteration of the poll loop: drain device events, expire timeouts.
    pub fn tick(&self) {
        let now = self.now_ms();

        // Drain device events without holding the lock across emits.
        let mut events: Vec<DeviceEvent> = Vec::new();
        let mut failure: Option<String> = None;
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(t) = inner.transport.as_mut() {
                loop {
                    match t.poll_event() {
                        Ok(Some(ev)) => events.push(ev),
                        Ok(None) => break,
                        Err(e) => {
                            failure = Some(e.to_string());
                            break;
                        }
                    }
                }
                if !events.is_empty() {
                    inner.last_io_ms = Some(now);
                }
            }
        }
        if let Some(msg) = failure {
            self.handle_transport_failure(&msg);
        }

        let mut queue_events: Vec<QueueEvent> = Vec::new();
        for ev in events {
            match ev {
                DeviceEvent::Ready => {
                    log::info!("device ready");
                    self.push_led();
                }
                DeviceEvent::Button { gesture } => {
                    self.emit(
                        EVT_BUTTON,
                        ButtonEventPayload {
                            gesture,
                            timestamp_ms: now,
                        },
                    );
                    queue_events
                        .extend(self.inner.lock().unwrap().queue.handle_button(gesture, now));
                }
            }
        }
        queue_events.extend(self.inner.lock().unwrap().queue.tick(now));

        let mut queue_changed = false;
        for qe in queue_events {
            match qe {
                QueueEvent::Resolved(res) => {
                    self.record_resolution(res);
                    queue_changed = true;
                }
                QueueEvent::Progress {
                    id,
                    clicks,
                    required,
                } => {
                    self.emit(
                        EVT_APPROVAL_PROGRESS,
                        serde_json::json!({ "id": id, "clicks": clicks, "required": required }),
                    );
                }
                QueueEvent::EmergencyStop => {
                    self.emit(
                        EVT_ERROR,
                        serde_json::json!({ "message": "emergency stop (very long press)" }),
                    );
                    let mut inner = self.inner.lock().unwrap();
                    inner.resume_state = (AgentState::Error, RiskLevel::None);
                    queue_changed = true;
                }
            }
        }
        if queue_changed {
            self.after_queue_change();
        }
    }
}
