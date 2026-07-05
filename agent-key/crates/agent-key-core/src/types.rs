//! Shared data types. All enums have a stable wire byte (`as u8`) used by the
//! binary protocol and a stable snake_case string form used by JSON APIs.

use serde::{Deserialize, Serialize};

/// High-level state of the coding agent, mirrored on the device LED.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum AgentState {
    #[default]
    Idle = 0,
    /// Agent is working / reasoning ("working"): soft blue breathing.
    Thinking = 1,
    /// A tool (shell command, edit, ...) is executing: yellow.
    ToolRunning = 2,
    /// Turn finished successfully: soft green.
    Done = 3,
    /// Waiting for human approval: red double blink.
    NeedsApproval = 4,
    /// Something went wrong: red fast blink.
    Error = 5,
    /// LED off.
    Off = 6,
}

impl AgentState {
    pub fn from_byte(b: u8) -> Option<Self> {
        use AgentState::*;
        Some(match b {
            0 => Idle,
            1 => Thinking,
            2 => ToolRunning,
            3 => Done,
            4 => NeedsApproval,
            5 => Error,
            6 => Off,
            _ => return None,
        })
    }
}

/// Risk classification of the action awaiting approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum RiskLevel {
    #[default]
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

impl RiskLevel {
    pub fn from_byte(b: u8) -> Option<Self> {
        use RiskLevel::*;
        Some(match b {
            0 => None,
            1 => Low,
            2 => Medium,
            3 => High,
            4 => Critical,
            _ => return Option::None,
        })
    }
}

/// LED animation pattern. The device combines this with the color derived
/// from [`AgentState`] (see docs/PROTOCOL.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum LedPattern {
    Off = 0,
    Solid = 1,
    /// Soft sine-ish breathing.
    #[default]
    Breath = 2,
    /// 1 Hz blink.
    Blink = 3,
    /// Two short flashes, then a pause.
    DoubleBlink = 4,
    /// Urgent ~5 Hz blink.
    FastBlink = 5,
}

/// Button gesture reported by the device (`B1` packet event byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ButtonGesture {
    /// Short single press: approve.
    Single = 1,
    /// Fast double press: show details (or 2 approval clicks when pending).
    Double = 2,
    /// Long press (>= 800 ms): deny.
    Long = 3,
    /// Very long press (>= 3 s): emergency stop.
    VeryLong = 4,
    /// Raw press-down (informational).
    Down = 5,
    /// Raw release (informational).
    Up = 6,
}

impl ButtonGesture {
    pub fn from_byte(b: u8) -> Option<Self> {
        use ButtonGesture::*;
        Some(match b {
            1 => Single,
            2 => Double,
            3 => Long,
            4 => VeryLong,
            5 => Down,
            6 => Up,
            _ => return None,
        })
    }
}

/// Events flowing device -> host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeviceEvent {
    Button { gesture: ButtonGesture },
    /// Sent by firmware once after boot / (re)enumeration.
    Ready,
}

impl DeviceEvent {
    pub fn to_byte(self) -> u8 {
        match self {
            DeviceEvent::Button { gesture } => gesture as u8,
            DeviceEvent::Ready => 0x10,
        }
    }

    pub fn from_byte(b: u8) -> Option<Self> {
        if b == 0x10 {
            return Some(DeviceEvent::Ready);
        }
        ButtonGesture::from_byte(b).map(|gesture| DeviceEvent::Button { gesture })
    }
}

/// A status update pushed by the agent (frontend, CLI or localhost API).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusUpdate {
    pub state: AgentState,
    #[serde(default)]
    pub risk: Option<RiskLevel>,
    /// Free-form context, forwarded to UI listeners; never touches the device.
    #[serde(default)]
    pub message: Option<String>,
}

/// An approval request. Callers describe the action; the decision is made
/// exclusively by [`crate::approval_queue::ApprovalQueue`] from button input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Unique id. Leave empty to let the queue assign one.
    #[serde(default)]
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub risk: RiskLevel,
    /// Auto-resolve as timed out after this many ms (default 60_000).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Who asked (e.g. "claude-code", "codex", "cli").
    #[serde(default)]
    pub source: Option<String>,
}

/// Final decision for an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approved,
    Denied,
    Cancelled,
    TimedOut,
    EmergencyStopped,
}

/// Resolution record emitted when a request leaves the queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalResolution {
    pub id: String,
    pub decision: Decision,
    #[serde(default)]
    pub reason: Option<String>,
}

/// A device that can be connected to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Stable identifier, e.g. "mock" or a serial port name ("COM5").
    pub id: String,
    pub name: String,
    /// "mock" | "serial" | "hid"
    pub transport: String,
    #[serde(default)]
    pub port: Option<String>,
}

/// Health snapshot of the plugin / device link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub connected: bool,
    #[serde(default)]
    pub device: Option<DeviceInfo>,
    /// ms since the last packet was successfully written or read.
    #[serde(default)]
    pub last_io_ms: Option<u64>,
}

/// Full state snapshot returned by `get_current_state`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentState {
    pub state: AgentState,
    pub risk: RiskLevel,
    pub brightness: u8,
    pub connected: bool,
    #[serde(default)]
    pub pending_approval: Option<ApprovalRequest>,
}
