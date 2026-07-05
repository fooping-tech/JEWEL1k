//! MockTransport: full protocol emulation without hardware.
//!
//! Every outgoing packet is logged (via `log::info!`) in both hex and
//! decoded form, and kept in `sent` for assertions. Button events are
//! injected with [`Transport::inject_event`] (wired to the `simulate`
//! command of the CLI / localhost API).

use super::{Transport, TransportError, TransportKind};
use crate::led_policy::color_for;
use crate::protocol::HostPacket;
use crate::types::DeviceEvent;
use std::collections::VecDeque;

#[derive(Debug, Default)]
pub struct MockTransport {
    connected: bool,
    /// Log of every packet sent, oldest first.
    pub sent: Vec<HostPacket>,
    events: VecDeque<DeviceEvent>,
}

impl MockTransport {
    pub fn new() -> Self {
        let mut t = Self {
            connected: true,
            sent: Vec::new(),
            events: VecDeque::new(),
        };
        // Firmware sends Ready right after enumeration; the mock does too.
        t.events.push_back(DeviceEvent::Ready);
        t
    }

    pub fn last_sent(&self) -> Option<&HostPacket> {
        self.sent.last()
    }
}

impl Transport for MockTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Mock
    }

    fn send_packet(&mut self, packet: &HostPacket) -> Result<(), TransportError> {
        if !self.connected {
            return Err(TransportError::NotConnected);
        }
        let bytes = packet.encode();
        let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02X}")).collect();
        let color = color_for(packet.state);
        log::info!(
            "[mock->JEWEL1k] {} | state={:?} risk={:?} pattern={:?} brightness={} color=#{:02X}{:02X}{:02X}",
            hex.join(" "),
            packet.state,
            packet.risk,
            packet.pattern,
            packet.brightness,
            color.r,
            color.g,
            color.b,
        );
        self.sent.push(*packet);
        Ok(())
    }

    fn poll_event(&mut self) -> Result<Option<DeviceEvent>, TransportError> {
        if !self.connected {
            return Err(TransportError::NotConnected);
        }
        Ok(self.events.pop_front())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn close(&mut self) {
        self.connected = false;
    }

    fn inject_event(&mut self, event: DeviceEvent) -> Result<(), TransportError> {
        if !self.connected {
            return Err(TransportError::NotConnected);
        }
        log::info!("[mock<-button] injected {event:?}");
        self.events.push_back(event);
        Ok(())
    }

    fn last_packet(&self) -> Option<HostPacket> {
        self.sent.last().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentState, ButtonGesture, LedPattern, RiskLevel};

    #[test]
    fn mock_logs_sent_packets() {
        let mut t = MockTransport::new();
        let p = HostPacket {
            state: AgentState::Thinking,
            risk: RiskLevel::None,
            pattern: LedPattern::Breath,
            brightness: 255,
        };
        t.send_packet(&p).unwrap();
        assert_eq!(t.sent.len(), 1);
        assert_eq!(t.last_sent(), Some(&p));
    }

    #[test]
    fn mock_emits_ready_then_injected_events() {
        let mut t = MockTransport::new();
        assert_eq!(t.poll_event().unwrap(), Some(DeviceEvent::Ready));
        assert_eq!(t.poll_event().unwrap(), None);
        t.inject_event(DeviceEvent::Button {
            gesture: ButtonGesture::Single,
        })
        .unwrap();
        assert_eq!(
            t.poll_event().unwrap(),
            Some(DeviceEvent::Button {
                gesture: ButtonGesture::Single
            })
        );
    }

    #[test]
    fn closed_mock_rejects_io() {
        let mut t = MockTransport::new();
        t.close();
        assert!(!t.is_connected());
        assert!(t
            .send_packet(&HostPacket {
                state: AgentState::Idle,
                risk: RiskLevel::None,
                pattern: LedPattern::Solid,
                brightness: 10,
            })
            .is_err());
    }
}
