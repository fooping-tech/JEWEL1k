//! agent-key-core
//!
//! Core building blocks for driving a JEWEL1k device as an AI-coding-agent
//! status LED + physical approval button:
//!
//! - [`types`]: shared data types (agent state, risk levels, approvals, events)
//! - [`protocol`]: binary wire protocol (`A1 ...` host->device, `B1 ...` device->host)
//! - [`led_policy`]: agent state -> LED pattern/color/brightness mapping
//! - [`risk_policy`]: risk level -> approval rules (auto-deny, long-press deny, ...)
//! - [`approval_queue`]: the single source of truth for approval decisions
//! - [`transport`]: `Transport` trait + `MockTransport` (+ `SerialTransport` with the
//!   `serial` feature, `HidRawTransport` with the `hid` feature)

pub mod approval_queue;
pub mod led_policy;
pub mod protocol;
pub mod risk_policy;
pub mod transport;
pub mod types;

pub use approval_queue::{ApprovalQueue, QueueEvent, SubmitOutcome};
pub use led_policy::{led_for, LedCommand};
pub use protocol::{Decoder, HostPacket, DEVICE_HEADER, HOST_HEADER};
pub use risk_policy::{rule_for, RiskRule};
pub use transport::{MockTransport, Transport, TransportError, TransportKind};
pub use types::*;
