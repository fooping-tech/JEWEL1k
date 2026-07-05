//! Risk level -> approval rule mapping (the safety policy).
//!
//! - none / low / medium : one click approves, long press denies
//! - high                : two clicks within 5 s approve, long press denies
//! - critical            : auto-denied, the button cannot approve it
//!
//! These rules are consumed exclusively by [`crate::approval_queue`]; neither
//! the frontend nor an LLM can override a decision.

use crate::types::RiskLevel;
use serde::{Deserialize, Serialize};

pub const HIGH_RISK_CLICK_WINDOW_MS: u64 = 5_000;
pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskRule {
    /// The request is denied immediately on submit; no button input applies.
    pub auto_deny: bool,
    /// Number of clicks required to approve.
    pub clicks_to_approve: u8,
    /// Clicks must all land within this window or the count resets.
    pub click_window_ms: u64,
    /// Whether a long press denies the request (always true today).
    pub long_press_denies: bool,
}

pub fn rule_for(risk: RiskLevel) -> RiskRule {
    match risk {
        RiskLevel::None | RiskLevel::Low | RiskLevel::Medium => RiskRule {
            auto_deny: false,
            clicks_to_approve: 1,
            click_window_ms: HIGH_RISK_CLICK_WINDOW_MS,
            long_press_denies: true,
        },
        RiskLevel::High => RiskRule {
            auto_deny: false,
            clicks_to_approve: 2,
            click_window_ms: HIGH_RISK_CLICK_WINDOW_MS,
            long_press_denies: true,
        },
        RiskLevel::Critical => RiskRule {
            auto_deny: true,
            clicks_to_approve: u8::MAX,
            click_window_ms: 0,
            long_press_denies: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medium_takes_one_click() {
        assert_eq!(rule_for(RiskLevel::Medium).clicks_to_approve, 1);
        assert!(!rule_for(RiskLevel::Medium).auto_deny);
    }

    #[test]
    fn high_takes_two_clicks_within_5s() {
        let r = rule_for(RiskLevel::High);
        assert_eq!(r.clicks_to_approve, 2);
        assert_eq!(r.click_window_ms, 5_000);
    }

    #[test]
    fn critical_is_auto_denied() {
        assert!(rule_for(RiskLevel::Critical).auto_deny);
    }
}
