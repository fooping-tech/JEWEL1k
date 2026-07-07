//! Risk level -> approval rule mapping (the safety policy).
//!
//! Approval is gesture-based and identical for every approvable risk level:
//!
//! - none / low / medium / high : **double press approves**, long press denies.
//!   A single press never approves anything (it stays available as a plain
//!   HID keystroke while no approval is pending).
//! - critical                   : auto-denied, the button cannot approve it
//!
//! These rules are consumed exclusively by [`crate::approval_queue`]; neither
//! the frontend nor an LLM can override a decision.

use crate::types::RiskLevel;
use serde::{Deserialize, Serialize};

pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskRule {
    /// The request is denied immediately on submit; no button input applies.
    pub auto_deny: bool,
    /// Whether a long press denies the request (always true today).
    pub long_press_denies: bool,
}

pub fn rule_for(risk: RiskLevel) -> RiskRule {
    match risk {
        RiskLevel::None | RiskLevel::Low | RiskLevel::Medium | RiskLevel::High => RiskRule {
            auto_deny: false,
            long_press_denies: true,
        },
        RiskLevel::Critical => RiskRule {
            auto_deny: true,
            long_press_denies: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approvable_levels_are_not_auto_denied() {
        for risk in [
            RiskLevel::None,
            RiskLevel::Low,
            RiskLevel::Medium,
            RiskLevel::High,
        ] {
            assert!(!rule_for(risk).auto_deny);
            assert!(rule_for(risk).long_press_denies);
        }
    }

    #[test]
    fn critical_is_auto_denied() {
        assert!(rule_for(RiskLevel::Critical).auto_deny);
    }
}
