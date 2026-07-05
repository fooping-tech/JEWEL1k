//! Maps agent state + risk to an LED command.
//!
//! The wire protocol only carries `state`, `risk`, `pattern`, `brightness`;
//! the firmware derives the color from `state` (same table as below). The
//! RGB value here is used for logs, mock output and future HID transports.

use crate::protocol::HostPacket;
use crate::types::{AgentState, LedPattern, RiskLevel};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedCommand {
    pub pattern: LedPattern,
    pub brightness: u8,
    /// Informational mirror of the color the firmware will show.
    pub color: Rgb,
}

/// Reference color table (must stay in sync with `stateColor()` in
/// src/agentkey/agentkey.ino and docs/PROTOCOL.md).
pub fn color_for(state: AgentState) -> Rgb {
    match state {
        AgentState::Idle => Rgb { r: 40, g: 40, b: 40 },      // dim white
        AgentState::Thinking => Rgb { r: 0, g: 60, b: 255 },  // blue
        AgentState::ToolRunning => Rgb { r: 255, g: 180, b: 0 }, // yellow
        AgentState::Done => Rgb { r: 0, g: 255, b: 60 },      // green
        AgentState::NeedsApproval => Rgb { r: 255, g: 0, b: 0 }, // red
        AgentState::Error => Rgb { r: 255, g: 0, b: 30 },     // red
        AgentState::Off => Rgb { r: 0, g: 0, b: 0 },
    }
}

/// Compute the LED command for a state/risk pair.
///
/// - thinking      -> soft blue breathing
/// - tool_running  -> solid yellow
/// - done          -> soft solid green
/// - needs_approval-> red double blink (fast blink when risk >= high)
/// - error         -> red fast blink
pub fn led_for(state: AgentState, risk: RiskLevel, master_brightness: u8) -> LedCommand {
    let (pattern, base): (LedPattern, u8) = match state {
        AgentState::Idle => (LedPattern::Solid, 40),
        AgentState::Thinking => (LedPattern::Breath, 255),
        AgentState::ToolRunning => (LedPattern::Solid, 200),
        AgentState::Done => (LedPattern::Solid, 140),
        AgentState::NeedsApproval => {
            if risk >= RiskLevel::High {
                (LedPattern::FastBlink, 255)
            } else {
                (LedPattern::DoubleBlink, 255)
            }
        }
        AgentState::Error => (LedPattern::FastBlink, 255),
        AgentState::Off => (LedPattern::Off, 0),
    };
    let brightness = ((base as u16 * master_brightness as u16) / 255) as u8;
    LedCommand {
        pattern,
        brightness,
        color: color_for(state),
    }
}

/// Convenience: build the host packet for a state/risk/master-brightness.
pub fn packet_for(state: AgentState, risk: RiskLevel, master_brightness: u8) -> HostPacket {
    let led = led_for(state, risk, master_brightness);
    HostPacket {
        state,
        risk,
        pattern: led.pattern,
        brightness: led.brightness,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_breathes_blue() {
        let led = led_for(AgentState::Thinking, RiskLevel::None, 255);
        assert_eq!(led.pattern, LedPattern::Breath);
        assert!(led.color.b > led.color.r && led.color.b > led.color.g);
    }

    #[test]
    fn approval_blinks_red_double_and_fast_for_high() {
        let med = led_for(AgentState::NeedsApproval, RiskLevel::Medium, 255);
        assert_eq!(med.pattern, LedPattern::DoubleBlink);
        let high = led_for(AgentState::NeedsApproval, RiskLevel::High, 255);
        assert_eq!(high.pattern, LedPattern::FastBlink);
        assert_eq!(med.color, Rgb { r: 255, g: 0, b: 0 });
    }

    #[test]
    fn master_brightness_scales() {
        let full = led_for(AgentState::ToolRunning, RiskLevel::None, 255);
        let half = led_for(AgentState::ToolRunning, RiskLevel::None, 128);
        assert!(half.brightness < full.brightness);
        assert_eq!(led_for(AgentState::Off, RiskLevel::None, 255).brightness, 0);
    }
}
