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

/// One connected device: its metadata plus the live transport. Several links
/// can be active at once; LED packets are broadcast to all of them and button
/// events from any of them feed the same approval queue.
struct Link {
    device: DeviceInfo,
    transport: Box<dyn Transport>,
}

struct Inner {
    links: Vec<Link>,
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
                links: Vec::new(),
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
            "mock" => {
                // `port` doubles as an id so tests / dev setups can attach
                // several mock devices side by side.
                let id = options.port.clone().unwrap_or_else(|| "mock".into());
                (
                    Box::new(MockTransport::new()),
                    DeviceInfo {
                        id,
                        name: "Mock JEWEL1k (no hardware)".into(),
                        transport: "mock".into(),
                        port: None,
                    },
                )
            }
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
            #[cfg(feature = "hid")]
            "hid" => {
                let t = agent_key_core::transport::HidRawTransport::open(
                    options.port.as_deref(),
                )?;
                let path = t.path().to_string();
                (
                    Box::new(t),
                    DeviceInfo {
                        id: path.clone(),
                        name: "JEWEL1k (raw HID)".into(),
                        transport: "hid".into(),
                        port: Some(path),
                    },
                )
            }
            other => {
                return Err(Error::UnknownDevice(other.to_string()));
            }
        };

        {
            let mut inner = self.inner.lock().unwrap();
            // Reconnecting the same device replaces its previous link.
            if let Some(pos) = inner.links.iter().position(|l| l.device.id == device.id) {
                let mut old = inner.links.remove(pos);
                old.transport.close();
            }
            inner.links.push(Link {
                device: device.clone(),
                transport,
            });
        }
        self.emit(EVT_DEVICE_CONNECTED, device.clone());
        self.push_led();
        Ok(device)
    }

    /// Disconnect one device by id, or every device when `id` is `None`.
    pub fn disconnect(&self, id: Option<&str>) -> Result<()> {
        let removed: Vec<DeviceInfo> = {
            let mut inner = self.inner.lock().unwrap();
            let mut removed = Vec::new();
            let mut i = 0;
            while i < inner.links.len() {
                if id.is_none() || id == Some(inner.links[i].device.id.as_str()) {
                    let mut link = inner.links.remove(i);
                    link.transport.close();
                    removed.push(link.device);
                } else {
                    i += 1;
                }
            }
            removed
        };
        if let Some(id) = id {
            if removed.is_empty() {
                return Err(Error::UnknownDevice(id.to_string()));
            }
        }
        for device in removed {
            self.emit(
                EVT_DEVICE_DISCONNECTED,
                serde_json::json!({ "device": device }),
            );
        }
        Ok(())
    }

    pub fn get_health(&self) -> Health {
        let inner = self.inner.lock().unwrap();
        Health {
            connected: inner.links.iter().any(|l| l.transport.is_connected()),
            device: inner.links.first().map(|l| l.device.clone()),
            devices: inner.links.iter().map(|l| l.device.clone()).collect(),
            last_io_ms: inner.last_io_ms.map(|t| self.now_ms().saturating_sub(t)),
        }
    }

    // ---- status / LED ------------------------------------------------

    pub fn set_status(&self, update: StatusUpdate) -> Result<CurrentState> {
        let mut superseded: Vec<ApprovalResolution> = Vec::new();
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(risk) = update.risk {
                inner.risk = risk;
            }
            if update.state == AgentState::NeedsApproval {
                // The needs_approval status is a transient overlay (e.g. the
                // Notification hook). Show it, but never record it as the
                // state to resume once the queue drains.
                inner.state = AgentState::NeedsApproval;
            } else {
                // The agent reports its own progress, so it is not parked on a
                // pending approval. In auto-accept mode nobody presses the
                // button, so an orphaned request would otherwise pin the LED
                // to needs_approval indefinitely. Supersede it — always as
                // Cancelled, never Approved, so the approval policy holds. In
                // the normal blocking flow the agent is stuck waiting on the
                // approval and emits no such status, so real gates survive.
                if !inner.queue.is_empty() {
                    superseded = inner
                        .queue
                        .cancel_all("superseded by a later agent status update");
                }
                inner.state = update.state;
                inner.resume_state = (update.state, inner.risk);
            }
            if !inner.queue.is_empty() {
                // keep showing needs_approval while requests are still pending
                inner.state = AgentState::NeedsApproval;
            }
        }
        for res in superseded {
            self.record_resolution(res);
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
            connected: inner.links.iter().any(|l| l.transport.is_connected()),
            pending_approval: inner.queue.current().cloned(),
        }
    }

    /// Broadcast the LED packet for the current state to every device.
    /// A failing link is dropped and reported; no links at all is fine
    /// (dev without device).
    fn push_led(&self) {
        let mut failures: Vec<(DeviceInfo, String)> = Vec::new();
        {
            let mut inner = self.inner.lock().unwrap();
            let packet = led_policy::packet_for(inner.state, inner.risk, inner.brightness);
            let now = self.now_ms();
            if inner.links.is_empty() {
                log::debug!("no transport connected; LED packet skipped: {packet:?}");
            }
            let mut sent = false;
            let mut i = 0;
            while i < inner.links.len() {
                match inner.links[i].transport.send_packet(&packet) {
                    Ok(()) => {
                        sent = true;
                        i += 1;
                    }
                    Err(e) => {
                        let mut link = inner.links.remove(i);
                        link.transport.close();
                        failures.push((link.device, e.to_string()));
                    }
                }
            }
            if sent {
                inner.last_io_ms = Some(now);
            }
        }
        for (device, msg) in failures {
            self.report_link_failure(device, &msg);
        }
    }

    fn report_link_failure(&self, device: DeviceInfo, msg: &str) {
        log::error!("transport failure on {}: {msg}", device.id);
        self.emit(
            EVT_ERROR,
            serde_json::json!({ "message": msg, "device": device.clone() }),
        );
        self.emit(
            EVT_DEVICE_DISCONNECTED,
            serde_json::json!({ "device": device }),
        );
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
                // Immediate policy resolution (e.g. critical -> denied) never
                // set NeedsApproval here, but a hook may have flipped the LED
                // red via /status just before submitting. Restore the agent's
                // own state so the red indicator does not linger.
                self.after_queue_change();
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

    /// Last LED packet written to a logging transport (mock introspection).
    pub fn last_sent_packet(&self) -> Option<agent_key_core::HostPacket> {
        self.inner
            .lock()
            .unwrap()
            .links
            .iter()
            .find_map(|l| l.transport.last_packet())
    }

    /// Same, but for one specific device id (multi-device introspection).
    pub fn last_sent_packet_for(&self, id: &str) -> Option<agent_key_core::HostPacket> {
        self.inner
            .lock()
            .unwrap()
            .links
            .iter()
            .find(|l| l.device.id == id)
            .and_then(|l| l.transport.last_packet())
    }

    // ---- device events / poll loop -------------------------------------

    pub fn simulate_button(&self, gesture: ButtonGesture) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.links.is_empty() {
            return Err(Error::NotConnected);
        }
        let mut last_err = Error::Transport(agent_key_core::TransportError::Unsupported);
        for link in inner.links.iter_mut() {
            match link.transport.inject_event(DeviceEvent::Button { gesture }) {
                Ok(()) => return Ok(()),
                Err(e) => last_err = Error::Transport(e),
            }
        }
        Err(last_err)
    }

    /// One iteration of the poll loop: drain device events, expire timeouts.
    pub fn tick(&self) {
        let now = self.now_ms();

        // Drain device events without holding the lock across emits.
        let mut events: Vec<DeviceEvent> = Vec::new();
        let mut failures: Vec<(DeviceInfo, String)> = Vec::new();
        {
            let mut inner = self.inner.lock().unwrap();
            let mut i = 0;
            while i < inner.links.len() {
                let mut failed: Option<String> = None;
                loop {
                    match inner.links[i].transport.poll_event() {
                        Ok(Some(ev)) => events.push(ev),
                        Ok(None) => break,
                        Err(e) => {
                            failed = Some(e.to_string());
                            break;
                        }
                    }
                }
                match failed {
                    Some(msg) => {
                        let mut link = inner.links.remove(i);
                        link.transport.close();
                        failures.push((link.device, msg));
                    }
                    None => i += 1,
                }
            }
            if !events.is_empty() {
                inner.last_io_ms = Some(now);
            }
        }
        for (device, msg) in failures {
            self.report_link_failure(device, &msg);
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
