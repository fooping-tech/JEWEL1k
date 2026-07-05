//! The approval queue: the ONLY component that turns button gestures into
//! approval decisions. Frontends, hooks and LLMs can submit or cancel
//! requests but can never resolve one as approved.
//!
//! Time is passed in explicitly (milliseconds from any monotonic origin) so
//! every rule is deterministic and unit-testable.

use crate::risk_policy::{rule_for, DEFAULT_TIMEOUT_MS};
use crate::types::{
    ApprovalRequest, ApprovalResolution, ButtonGesture, Decision, RiskLevel,
};
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq)]
pub enum QueueEvent {
    /// A request left the queue with a final decision.
    Resolved(ApprovalResolution),
    /// A click was registered but more are required (high risk).
    Progress {
        id: String,
        clicks: u8,
        required: u8,
    },
    /// A very long press triggered an emergency stop (all requests denied).
    EmergencyStop,
}

/// Result of submitting a request.
#[derive(Debug, Clone, PartialEq)]
pub enum SubmitOutcome {
    /// Queued and waiting for button input.
    Pending { id: String },
    /// Resolved immediately by policy (e.g. critical -> denied).
    Resolved(ApprovalResolution),
}

#[derive(Debug, Clone)]
struct Pending {
    request: ApprovalRequest,
    created_ms: u64,
    clicks: u8,
    first_click_ms: Option<u64>,
}

#[derive(Debug, Default)]
pub struct ApprovalQueue {
    pending: VecDeque<Pending>,
    next_id: u64,
}

impl ApprovalQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// The request currently owning the button (front of the queue).
    pub fn current(&self) -> Option<&ApprovalRequest> {
        self.pending.front().map(|p| &p.request)
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Submit a request. Critical risk is denied immediately (default-deny
    /// policy); everything else is queued FIFO.
    pub fn submit(&mut self, mut request: ApprovalRequest, now_ms: u64) -> SubmitOutcome {
        if request.id.is_empty() {
            self.next_id += 1;
            request.id = format!("apr-{}", self.next_id);
        }
        let rule = rule_for(request.risk);
        if rule.auto_deny {
            return SubmitOutcome::Resolved(ApprovalResolution {
                id: request.id,
                decision: Decision::Denied,
                reason: Some("critical risk is denied by policy (default deny)".into()),
            });
        }
        let id = request.id.clone();
        self.pending.push_back(Pending {
            request,
            created_ms: now_ms,
            clicks: 0,
            first_click_ms: None,
        });
        SubmitOutcome::Pending { id }
    }

    /// Cancel a pending request by id (e.g. the agent no longer needs it).
    pub fn cancel(&mut self, id: &str) -> Option<QueueEvent> {
        let idx = self.pending.iter().position(|p| p.request.id == id)?;
        let p = self.pending.remove(idx).expect("index just found");
        Some(QueueEvent::Resolved(ApprovalResolution {
            id: p.request.id,
            decision: Decision::Cancelled,
            reason: Some("cancelled by requester".into()),
        }))
    }

    /// Feed a button gesture. Returns zero or more queue events; gestures
    /// with no pending request return an empty vec (callers still forward
    /// the raw button event to listeners).
    pub fn handle_button(&mut self, gesture: ButtonGesture, now_ms: u64) -> Vec<QueueEvent> {
        if gesture == ButtonGesture::VeryLong {
            // Emergency stop applies even with an empty queue.
            let mut events: Vec<QueueEvent> = self
                .pending
                .drain(..)
                .map(|p| {
                    QueueEvent::Resolved(ApprovalResolution {
                        id: p.request.id,
                        decision: Decision::EmergencyStopped,
                        reason: Some("emergency stop (very long press)".into()),
                    })
                })
                .collect();
            events.push(QueueEvent::EmergencyStop);
            return events;
        }

        let Some(front) = self.pending.front_mut() else {
            return Vec::new();
        };
        let rule = rule_for(front.request.risk);

        match gesture {
            ButtonGesture::Single | ButtonGesture::Double => {
                // A fast double press counts as two clicks.
                let add = if gesture == ButtonGesture::Double { 2 } else { 1 };
                if let Some(first) = front.first_click_ms {
                    if now_ms.saturating_sub(first) > rule.click_window_ms {
                        front.clicks = 0;
                        front.first_click_ms = None;
                    }
                }
                if front.first_click_ms.is_none() {
                    front.first_click_ms = Some(now_ms);
                }
                front.clicks = front.clicks.saturating_add(add);
                if front.clicks >= rule.clicks_to_approve {
                    let p = self.pending.pop_front().expect("front exists");
                    vec![QueueEvent::Resolved(ApprovalResolution {
                        id: p.request.id,
                        decision: Decision::Approved,
                        reason: None,
                    })]
                } else {
                    vec![QueueEvent::Progress {
                        id: front.request.id.clone(),
                        clicks: front.clicks,
                        required: rule.clicks_to_approve,
                    }]
                }
            }
            ButtonGesture::Long if rule.long_press_denies => {
                let p = self.pending.pop_front().expect("front exists");
                vec![QueueEvent::Resolved(ApprovalResolution {
                    id: p.request.id,
                    decision: Decision::Denied,
                    reason: Some("denied by long press".into()),
                })]
            }
            _ => Vec::new(),
        }
    }

    /// Advance time: expire requests whose timeout elapsed and reset stale
    /// click counts. Call this periodically (e.g. every poll tick).
    pub fn tick(&mut self, now_ms: u64) -> Vec<QueueEvent> {
        let mut events = Vec::new();
        // Expire timeouts (any position in the queue).
        let mut i = 0;
        while i < self.pending.len() {
            let timeout = self.pending[i]
                .request
                .timeout_ms
                .unwrap_or(DEFAULT_TIMEOUT_MS);
            if now_ms.saturating_sub(self.pending[i].created_ms) >= timeout {
                let p = self.pending.remove(i).expect("index in range");
                events.push(QueueEvent::Resolved(ApprovalResolution {
                    id: p.request.id,
                    decision: Decision::TimedOut,
                    reason: Some("no button input before timeout".into()),
                }));
            } else {
                i += 1;
            }
        }
        // Reset expired click windows on the active request.
        if let Some(front) = self.pending.front_mut() {
            let rule = rule_for(front.request.risk);
            if let Some(first) = front.first_click_ms {
                if now_ms.saturating_sub(first) > rule.click_window_ms {
                    front.clicks = 0;
                    front.first_click_ms = None;
                }
            }
        }
        events
    }
}

/// Helper for tests and callers building requests.
pub fn request(title: &str, risk: RiskLevel) -> ApprovalRequest {
    ApprovalRequest {
        id: String::new(),
        title: title.to_string(),
        description: None,
        risk,
        timeout_ms: None,
        source: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(events: &[QueueEvent]) -> Option<&ApprovalResolution> {
        events.iter().find_map(|e| match e {
            QueueEvent::Resolved(r) => Some(r),
            _ => None,
        })
    }

    #[test]
    fn medium_risk_is_approved_by_single_click() {
        let mut q = ApprovalQueue::new();
        let SubmitOutcome::Pending { id } = q.submit(request("rm build dir", RiskLevel::Medium), 0)
        else {
            panic!("medium must queue");
        };
        let events = q.handle_button(ButtonGesture::Single, 100);
        let r = resolved(&events).expect("resolved");
        assert_eq!(r.id, id);
        assert_eq!(r.decision, Decision::Approved);
        assert!(q.is_empty());
    }

    #[test]
    fn medium_risk_is_denied_by_long_press() {
        let mut q = ApprovalQueue::new();
        q.submit(request("edit config", RiskLevel::Medium), 0);
        let events = q.handle_button(ButtonGesture::Long, 100);
        assert_eq!(resolved(&events).unwrap().decision, Decision::Denied);
    }

    #[test]
    fn high_risk_needs_two_clicks_within_5s() {
        let mut q = ApprovalQueue::new();
        q.submit(request("git push --force", RiskLevel::High), 0);

        let events = q.handle_button(ButtonGesture::Single, 1_000);
        assert!(matches!(
            events[0],
            QueueEvent::Progress {
                clicks: 1,
                required: 2,
                ..
            }
        ));

        let events = q.handle_button(ButtonGesture::Single, 3_000); // within 5 s
        assert_eq!(resolved(&events).unwrap().decision, Decision::Approved);
    }

    #[test]
    fn high_risk_click_window_expires() {
        let mut q = ApprovalQueue::new();
        q.submit(request("deploy prod", RiskLevel::High), 0);
        q.handle_button(ButtonGesture::Single, 1_000);
        // second click 6 s later -> window expired, counts as first click again
        let events = q.handle_button(ButtonGesture::Single, 7_000);
        assert!(matches!(
            events[0],
            QueueEvent::Progress {
                clicks: 1,
                required: 2,
                ..
            }
        ));
        // fast double press then approves
        let events = q.handle_button(ButtonGesture::Double, 8_000);
        assert_eq!(resolved(&events).unwrap().decision, Decision::Approved);
    }

    #[test]
    fn critical_risk_is_auto_denied() {
        let mut q = ApprovalQueue::new();
        let outcome = q.submit(request("rm -rf /", RiskLevel::Critical), 0);
        let SubmitOutcome::Resolved(r) = outcome else {
            panic!("critical must resolve immediately");
        };
        assert_eq!(r.decision, Decision::Denied);
        assert!(q.is_empty());
        // and the button can never approve it
        assert!(q.handle_button(ButtonGesture::Single, 10).is_empty());
    }

    #[test]
    fn very_long_press_emergency_stops_everything() {
        let mut q = ApprovalQueue::new();
        q.submit(request("a", RiskLevel::Medium), 0);
        q.submit(request("b", RiskLevel::High), 0);
        let events = q.handle_button(ButtonGesture::VeryLong, 500);
        let stops = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    QueueEvent::Resolved(ApprovalResolution {
                        decision: Decision::EmergencyStopped,
                        ..
                    })
                )
            })
            .count();
        assert_eq!(stops, 2);
        assert!(events.contains(&QueueEvent::EmergencyStop));
        assert!(q.is_empty());
    }

    #[test]
    fn requests_time_out() {
        let mut q = ApprovalQueue::new();
        let mut req = request("slow", RiskLevel::Medium);
        req.timeout_ms = Some(1_000);
        q.submit(req, 0);
        assert!(q.tick(500).is_empty());
        let events = q.tick(1_000);
        assert_eq!(resolved(&events).unwrap().decision, Decision::TimedOut);
    }

    #[test]
    fn cancel_removes_pending() {
        let mut q = ApprovalQueue::new();
        let SubmitOutcome::Pending { id } = q.submit(request("x", RiskLevel::Low), 0) else {
            panic!()
        };
        let ev = q.cancel(&id).expect("cancelled");
        assert!(matches!(
            ev,
            QueueEvent::Resolved(ApprovalResolution {
                decision: Decision::Cancelled,
                ..
            })
        ));
        assert!(q.cancel(&id).is_none());
    }

    #[test]
    fn fifo_order_second_request_becomes_active() {
        let mut q = ApprovalQueue::new();
        let SubmitOutcome::Pending { id: a } = q.submit(request("a", RiskLevel::Medium), 0) else {
            panic!()
        };
        let SubmitOutcome::Pending { id: b } = q.submit(request("b", RiskLevel::Medium), 0) else {
            panic!()
        };
        let events = q.handle_button(ButtonGesture::Single, 10);
        assert_eq!(resolved(&events).unwrap().id, a);
        assert_eq!(q.current().unwrap().id, b);
    }
}
