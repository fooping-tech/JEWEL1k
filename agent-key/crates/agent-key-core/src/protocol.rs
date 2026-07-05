//! Binary wire protocol between the host and the JEWEL1k (CH552E).
//!
//! Host -> device (6 bytes):  `A1 state risk pattern brightness checksum`
//! Device -> host (3 bytes):  `B1 event checksum`
//!
//! `checksum` is the XOR of all preceding bytes (header included).
//! See docs/PROTOCOL.md for the full specification.

use crate::types::{AgentState, DeviceEvent, LedPattern, RiskLevel};
use serde::{Deserialize, Serialize};

pub const HOST_HEADER: u8 = 0xA1;
pub const DEVICE_HEADER: u8 = 0xB1;
pub const HOST_PACKET_LEN: usize = 6;
pub const DEVICE_PACKET_LEN: usize = 3;

/// The logical content of a host -> device packet. Kept as a serde struct
/// internally; converted to bytes only at the transport boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostPacket {
    pub state: AgentState,
    pub risk: RiskLevel,
    pub pattern: LedPattern,
    pub brightness: u8,
}

fn xor(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |acc, b| acc ^ b)
}

impl HostPacket {
    pub fn encode(&self) -> [u8; HOST_PACKET_LEN] {
        let mut buf = [
            HOST_HEADER,
            self.state as u8,
            self.risk as u8,
            self.pattern as u8,
            self.brightness,
            0,
        ];
        buf[5] = xor(&buf[..5]);
        buf
    }

    pub fn decode(buf: &[u8]) -> Result<Self, ProtocolError> {
        if buf.len() != HOST_PACKET_LEN {
            return Err(ProtocolError::Length);
        }
        if buf[0] != HOST_HEADER {
            return Err(ProtocolError::Header(buf[0]));
        }
        if xor(&buf[..5]) != buf[5] {
            return Err(ProtocolError::Checksum);
        }
        Ok(HostPacket {
            state: AgentState::from_byte(buf[1]).ok_or(ProtocolError::Field("state"))?,
            risk: RiskLevel::from_byte(buf[2]).ok_or(ProtocolError::Field("risk"))?,
            pattern: match buf[3] {
                0 => LedPattern::Off,
                1 => LedPattern::Solid,
                2 => LedPattern::Breath,
                3 => LedPattern::Blink,
                4 => LedPattern::DoubleBlink,
                5 => LedPattern::FastBlink,
                _ => return Err(ProtocolError::Field("pattern")),
            },
            brightness: buf[4],
        })
    }
}

/// Encode a device -> host event (used by tests and by MockTransport to
/// emulate firmware output).
pub fn encode_event(event: DeviceEvent) -> [u8; DEVICE_PACKET_LEN] {
    let e = event.to_byte();
    [DEVICE_HEADER, e, DEVICE_HEADER ^ e]
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("bad packet length")]
    Length,
    #[error("unexpected header byte 0x{0:02X}")]
    Header(u8),
    #[error("checksum mismatch")]
    Checksum,
    #[error("invalid value for field `{0}`")]
    Field(&'static str),
}

/// Incremental decoder for the device -> host byte stream. Feed arbitrary
/// chunks; it re-synchronizes on the `B1` header and silently drops corrupt
/// frames (they are counted in `bad_frames`).
#[derive(Debug, Default)]
pub struct Decoder {
    buf: Vec<u8>,
    pub bad_frames: u32,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<DeviceEvent> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            // Re-sync: drop leading garbage until a header byte.
            while !self.buf.is_empty() && self.buf[0] != DEVICE_HEADER {
                self.buf.remove(0);
                self.bad_frames += 1;
            }
            if self.buf.len() < DEVICE_PACKET_LEN {
                return out;
            }
            let frame: [u8; DEVICE_PACKET_LEN] =
                [self.buf[0], self.buf[1], self.buf[2]];
            self.buf.drain(..DEVICE_PACKET_LEN);
            if frame[0] ^ frame[1] != frame[2] {
                self.bad_frames += 1;
                continue;
            }
            match DeviceEvent::from_byte(frame[1]) {
                Some(ev) => out.push(ev),
                None => self.bad_frames += 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ButtonGesture;

    #[test]
    fn host_packet_roundtrip() {
        let p = HostPacket {
            state: AgentState::Thinking,
            risk: RiskLevel::Medium,
            pattern: LedPattern::Breath,
            brightness: 200,
        };
        let bytes = p.encode();
        assert_eq!(bytes[0], 0xA1);
        assert_eq!(bytes[5], 0xA1 ^ 1 ^ 2 ^ 2 ^ 200);
        assert_eq!(HostPacket::decode(&bytes).unwrap(), p);
    }

    #[test]
    fn host_packet_rejects_bad_checksum() {
        let mut bytes = HostPacket {
            state: AgentState::Done,
            risk: RiskLevel::None,
            pattern: LedPattern::Solid,
            brightness: 10,
        }
        .encode();
        bytes[5] ^= 0xFF;
        assert_eq!(HostPacket::decode(&bytes), Err(ProtocolError::Checksum));
    }

    #[test]
    fn decoder_parses_stream_with_garbage_and_partial_frames() {
        let mut dec = Decoder::new();
        let single = encode_event(DeviceEvent::Button {
            gesture: ButtonGesture::Single,
        });
        let long = encode_event(DeviceEvent::Button {
            gesture: ButtonGesture::Long,
        });

        // garbage + one frame + half a frame
        let mut stream = vec![0x00, 0xFF];
        stream.extend_from_slice(&single);
        stream.extend_from_slice(&long[..1]);
        let events = dec.feed(&stream);
        assert_eq!(
            events,
            vec![DeviceEvent::Button {
                gesture: ButtonGesture::Single
            }]
        );

        // rest of the second frame
        let events = dec.feed(&long[1..]);
        assert_eq!(
            events,
            vec![DeviceEvent::Button {
                gesture: ButtonGesture::Long
            }]
        );
        assert_eq!(dec.bad_frames, 2); // the two garbage bytes
    }

    #[test]
    fn decoder_drops_corrupt_frame_and_recovers() {
        let mut dec = Decoder::new();
        let mut bad = encode_event(DeviceEvent::Ready);
        bad[2] ^= 0x01; // corrupt checksum
        let good = encode_event(DeviceEvent::Ready);
        let mut stream = bad.to_vec();
        stream.extend_from_slice(&good);
        let events = dec.feed(&stream);
        assert_eq!(events, vec![DeviceEvent::Ready]);
        assert!(dec.bad_frames >= 1);
    }
}
