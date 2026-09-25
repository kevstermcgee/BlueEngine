//! Reliable command delivery over unreliable datagram transports.
//!
//! Clients enqueue commands with strictly increasing sequence numbers.
//! Unacknowledged commands are resent periodically until the server's snapshot acknowledges them.
use std::collections::VecDeque;

/// A sequenced command wrapper.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SequencedCommand<T> {
    pub sequence: u64,
    pub command: T,
}

/// Client-side bounded queue of reliable commands waiting for server acknowledgement.
pub struct ReliableCommandQueue<T> {
    pending: VecDeque<SequencedCommand<T>>,
    next_sequence: u64,
    max_capacity: usize,
}

impl<T: Clone> ReliableCommandQueue<T> {
    pub fn new(max_capacity: usize) -> Self {
        Self {
            pending: VecDeque::with_capacity(max_capacity),
            next_sequence: 1,
            max_capacity: max_capacity.max(1),
        }
    }

    /// Enqueue a new command if under capacity limit. Returns the assigned sequence number.
    pub fn push(&mut self, command: T) -> Option<u64> {
        if self.pending.len() >= self.max_capacity {
            return None;
        }
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.pending
            .push_back(SequencedCommand { sequence, command });
        Some(sequence)
    }

    /// The oldest unacknowledged command to transmit or retransmit.
    pub fn front(&self) -> Option<&SequencedCommand<T>> {
        self.pending.front()
    }

    /// All currently unacknowledged commands.
    pub fn iter(&self) -> impl Iterator<Item = &SequencedCommand<T>> {
        self.pending.iter()
    }

    /// Acknowledge all commands up to and including `ack_sequence`.
    pub fn acknowledge(&mut self, ack_sequence: u64) {
        self.pending.retain(|cmd| cmd.sequence > ack_sequence);
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

impl<T: Clone> Default for ReliableCommandQueue<T> {
    fn default() -> Self {
        Self::new(16)
    }
}
