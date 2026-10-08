//! The download state machine: which status changes are allowed.
//!
//! ```text
//!   Queued ──► Downloading ──► Completed ──► Queued (restart)
//!     ▲  │          │  │
//!     │  ▼          ▼  ▼
//!   Paused ◄───── (pause)  Error ──► Queued (retry)
//! ```

use crate::manager::Status;

/// True when a download may move from `from` to `to`. Staying in the same state is allowed.
pub fn transition_allowed(from: Status, to: Status) -> bool {
    use Status::*;
    from == to
        || matches!(
            (from, to),
            (Queued, Downloading)
                | (Queued, Paused)
                | (Downloading, Paused)
                | (Downloading, Completed)
                | (Downloading, Error)
                | (Paused, Queued)
                | (Error, Queued)
                | (Completed, Queued)
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use Status::*;

    #[test]
    fn normal_life_cycle() {
        assert!(transition_allowed(Queued, Downloading));
        assert!(transition_allowed(Downloading, Paused));
        assert!(transition_allowed(Paused, Queued));
        assert!(transition_allowed(Downloading, Completed));
        assert!(transition_allowed(Error, Queued));
        assert!(transition_allowed(Completed, Queued));
    }

    #[test]
    fn impossible_jumps_are_refused() {
        assert!(!transition_allowed(Completed, Downloading));
        assert!(!transition_allowed(Completed, Paused));
        assert!(!transition_allowed(Paused, Completed));
        assert!(!transition_allowed(Paused, Downloading));
        assert!(!transition_allowed(Queued, Completed));
        assert!(!transition_allowed(Error, Completed));
    }

    #[test]
    fn same_state_is_a_no_op() {
        for s in [Queued, Downloading, Paused, Completed, Error] {
            assert!(transition_allowed(s, s));
        }
    }
}
