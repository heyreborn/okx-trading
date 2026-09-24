//! Process liveness and explicit readiness. Health is a diagnostic view of
//! caller-supplied facts; a green status never lifts a trading pause.

use crate::log::Reason;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Current process health snapshot. Readiness includes the last pause reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub live: bool,
    pub ready: bool,
    pub reason: Option<Reason>,
}

/// Health state owned by one process. Starts unready until dependencies and
/// source facts are explicitly checked by its caller.
pub struct Health {
    live: AtomicBool,
    reason: Mutex<Option<Reason>>,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            live: AtomicBool::new(true),
            reason: Mutex::new(Some(Reason::DependencyUnavailable)),
        }
    }
}

impl Health {
    /// Marks the process unready; this has no trading or persistence side effect.
    pub fn pause(&self, reason: Reason) {
        let mut current = self
            .reason
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *current = Some(if self.live.load(Ordering::Acquire) {
            reason
        } else {
            Reason::Shutdown
        });
    }

    /// Marks the process ready after its owner explicitly rechecks needed facts.
    /// This does not approve orders or clear domain-specific trading pauses.
    pub fn mark_ready(&self) {
        let mut reason = self
            .reason
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if self.live.load(Ordering::Acquire) {
            *reason = None;
        }
    }

    /// Marks process shutdown; no later readiness change restores liveness.
    pub fn stop(&self) {
        self.live.store(false, Ordering::Release);
        self.pause(Reason::Shutdown);
    }

    /// Reads a consistent diagnostic snapshot; does not probe dependencies.
    pub fn snapshot(&self) -> Snapshot {
        let reason = *self
            .reason
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let live = self.live.load(Ordering::Acquire);
        Snapshot {
            live,
            ready: live && reason.is_none(),
            reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_requires_explicit_transition_and_stop_is_terminal() {
        let health = Health::default();
        assert!(!health.snapshot().ready);
        health.mark_ready();
        assert!(health.snapshot().ready);
        health.pause(Reason::FactsStale);
        assert_eq!(health.snapshot().reason, Some(Reason::FactsStale));
        health.stop();
        health.mark_ready();
        health.pause(Reason::FactsStale);
        assert!(!health.snapshot().live);
        assert!(!health.snapshot().ready);
        assert_eq!(health.snapshot().reason, Some(Reason::Shutdown));
    }
}
