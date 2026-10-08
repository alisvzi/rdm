//! Download queues: each queue has its own limits, priority, schedule and finish action.
//! The choice of "which queue may start the next download" is a pure function with tests.

use serde::{Deserialize, Serialize};

/// The queue every download belongs to unless the user picks another.
pub const MAIN_QUEUE: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Queue {
    pub id: u32,
    pub name: String,
    /// A disabled queue keeps its waiting downloads but starts none of them.
    pub enabled: bool,
    /// Higher number = starts first when several queues have waiting downloads.
    pub priority: i32,
    /// Downloads of this queue running at the same time (also limited by the global maximum).
    pub max_concurrent: usize,
    /// Speed limit shared by all downloads of this queue, KiB/s. 0 = unlimited.
    pub speed_limit_kbps: u64,
    pub schedule_enabled: bool,
    /// "HH:MM", empty = no start time.
    pub schedule_start: String,
    pub schedule_stop: String,
    /// "none" | "shutdown" | "hibernate" after a scheduled run of this queue finishes.
    pub after_finish: String,
    /// Run `after_finish` only when every download of the run succeeded (no errors).
    pub only_if_all_succeeded: bool,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            id: MAIN_QUEUE,
            name: "Main".to_string(),
            enabled: true,
            priority: 0,
            max_concurrent: 3,
            speed_limit_kbps: 0,
            schedule_enabled: false,
            schedule_start: String::new(),
            schedule_stop: String::new(),
            after_finish: "none".to_string(),
            only_if_all_succeeded: true,
        }
    }
}

impl Queue {
    pub fn sanitize(&mut self) {
        self.name = self.name.trim().chars().take(40).collect();
        if self.name.is_empty() {
            self.name = format!("Queue {}", self.id);
        }
        self.priority = self.priority.clamp(-100, 100);
        self.max_concurrent = self.max_concurrent.clamp(1, 16);
        if !crate::manager::valid_hm(&self.schedule_start) {
            self.schedule_start.clear();
        }
        if !crate::manager::valid_hm(&self.schedule_stop) {
            self.schedule_stop.clear();
        }
        if !matches!(self.after_finish.as_str(), "shutdown" | "hibernate") {
            self.after_finish = "none".to_string();
        }
    }
}

/// The part of a queue that matters for choosing what starts next.
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub id: u32,
    pub priority: i32,
    pub max: usize,
    pub enabled: bool,
}

/// Which queue may start its next waiting download now?
/// `running` = (queue id, downloads running in it), `waiting` = ids of queues that have at
/// least one waiting download. Highest priority wins; equal priority → lowest queue id.
pub fn pick_queue(slots: &[Slot], running: &[(u32, usize)], waiting: &[u32]) -> Option<u32> {
    let count = |id: u32| {
        running
            .iter()
            .filter(|(q, _)| *q == id)
            .map(|(_, n)| *n)
            .sum::<usize>()
    };
    slots
        .iter()
        .filter(|s| s.enabled && waiting.contains(&s.id) && count(s.id) < s.max)
        .min_by_key(|s| (-(s.priority as i64), s.id))
        .map(|s| s.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(id: u32, priority: i32, max: usize, enabled: bool) -> Slot {
        Slot {
            id,
            priority,
            max,
            enabled,
        }
    }

    #[test]
    fn higher_priority_first() {
        let s = [slot(1, 0, 3, true), slot(2, 5, 3, true)];
        assert_eq!(pick_queue(&s, &[], &[1, 2]), Some(2));
    }

    #[test]
    fn equal_priority_uses_lowest_id() {
        let s = [slot(2, 0, 3, true), slot(1, 0, 3, true)];
        assert_eq!(pick_queue(&s, &[], &[1, 2]), Some(1));
    }

    #[test]
    fn full_queue_is_skipped() {
        let s = [slot(1, 9, 2, true), slot(2, 0, 2, true)];
        assert_eq!(pick_queue(&s, &[(1, 2)], &[1, 2]), Some(2));
    }

    #[test]
    fn disabled_or_empty_queues_never_start() {
        let s = [slot(1, 9, 2, false), slot(2, 0, 2, true)];
        assert_eq!(pick_queue(&s, &[], &[1]), None);
        assert_eq!(pick_queue(&s, &[], &[2]), Some(2));
        assert_eq!(pick_queue(&s, &[], &[]), None);
    }
}
