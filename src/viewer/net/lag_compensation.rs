//! Authoritative server lag compensation and bounded historical pose recording.
//!
//! When a client fires a weapon, it sends the `aim_tick` it was viewing on its screen.
//! The server clamps this tick to its allowed history window (e.g. 200 ms / 12 ticks)
//! and rewinds the target entities to their historical poses for fair hit detection.
use std::collections::VecDeque;

/// Bounded ring buffer of historical entity poses indexed by simulation tick.
#[derive(Clone, Debug)]
pub struct PoseHistory<T> {
    buffer: VecDeque<(u64, T)>,
    capacity: usize,
}

impl<T: Clone> PoseHistory<T> {
    /// Create a new pose history buffer storing up to `capacity` snapshots.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity: capacity.max(1),
        }
    }

    /// Record entity pose at `tick`. Evicts oldest record when capacity is exceeded.
    pub fn record(&mut self, tick: u64, state: T) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back((tick, state));
    }

    /// Retrieve the recorded pose at or immediately before `tick`.
    pub fn at_or_before(&self, tick: u64) -> Option<&T> {
        self.buffer
            .iter()
            .rev()
            .find(|(t, _)| *t <= tick)
            .map(|(_, s)| s)
    }

    /// Query the historical pose with strict server clamping.
    ///
    /// Clamps `requested_tick` to `[current_tick.saturating_sub(max_rewind), current_tick]`,
    /// then finds the recorded pose at or immediately before the clamped tick.
    pub fn clamped(&self, current_tick: u64, requested_tick: u64, max_rewind: u64) -> Option<&T> {
        let min_allowed = current_tick.saturating_sub(max_rewind);
        let clamped_tick = requested_tick.clamp(min_allowed, current_tick);
        self.at_or_before(clamped_tick)
    }

    pub fn latest(&self) -> Option<&T> {
        self.buffer.back().map(|(_, s)| s)
    }

    pub fn latest_tick(&self) -> Option<u64> {
        self.buffer.back().map(|(t, _)| *t)
    }

    pub fn oldest_tick(&self) -> Option<u64> {
        self.buffer.front().map(|(t, _)| *t)
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pose_history_records_and_evicts() {
        let mut history = PoseHistory::new(3);
        history.record(10, 100);
        history.record(11, 101);
        history.record(12, 102);
        assert_eq!(history.len(), 3);
        assert_eq!(history.oldest_tick(), Some(10));
        assert_eq!(history.latest_tick(), Some(12));

        history.record(13, 103);
        assert_eq!(history.len(), 3);
        assert_eq!(history.oldest_tick(), Some(11));
        assert_eq!(history.latest_tick(), Some(13));
    }

    #[test]
    fn pose_history_at_or_before_and_clamping() {
        let mut history = PoseHistory::new(10);
        history.record(100, "pose-100");
        history.record(105, "pose-105");
        history.record(110, "pose-110");

        assert_eq!(history.at_or_before(104), Some(&"pose-100"));
        assert_eq!(history.at_or_before(105), Some(&"pose-105"));
        assert_eq!(history.at_or_before(109), Some(&"pose-105"));
        assert_eq!(history.at_or_before(110), Some(&"pose-110"));
        assert_eq!(history.at_or_before(99), None);

        // Clamping with current_tick = 110, max_rewind = 8 (allowed range 102..110)
        // Request tick 100 -> clamped to 102 -> finds pose at or before 102 which is pose-100
        assert_eq!(history.clamped(110, 100, 8), Some(&"pose-100"));

        // Clamping with current_tick = 110, max_rewind = 4 (allowed range 106..110)
        // Request tick 90 -> clamped to 106 -> finds pose at or before 106 which is pose-105
        assert_eq!(history.clamped(110, 90, 4), Some(&"pose-105"));
    }
}
