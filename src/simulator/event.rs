//! Event queue with deterministic ordering.
//!
//! Events are ordered by `(timestamp, sequence_number)`.  The sequence number
//! breaks ties when multiple events share a timestamp, ensuring FIFO order
//! among same-time insertions.  This makes simulation runs fully reproducible
//! given the same seed.
//!
//! `TimestampedEvent` has a custom `Ord` (reversed, for min-heap via
//! `BinaryHeap`) and a custom `PartialEq` that compares only `(timestamp,
//! sequence_number)`, ignoring the event payload.  This is intentional: the
//! heap only needs ordering identity, and the payload is consumed on pop.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::types::{ActorId, Message};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalEvent {
    StartTransaction,
    Tick { to: ActorId },
    TickAll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InternalEvent {
    Deliver { to: ActorId, msg: Message },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    External(ExternalEvent),
    Internal(InternalEvent),
}

struct TimestampedEvent {
    timestamp: u64,
    sequence_number: u64,
    event: Event,
}

// Reversed ordering to make BinaryHeap behave as a min-heap.
impl Ord for TimestampedEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .timestamp
            .cmp(&self.timestamp)
            .then(other.sequence_number.cmp(&self.sequence_number))
    }
}

impl PartialOrd for TimestampedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for TimestampedEvent {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp && self.sequence_number == other.sequence_number
    }
}

impl Eq for TimestampedEvent {}

pub(crate) struct EventQueue {
    queue: BinaryHeap<TimestampedEvent>,
    next_sequence_number: u64,
}

impl EventQueue {
    pub(crate) fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            next_sequence_number: 0,
        }
    }

    pub(crate) fn insert(&mut self, timestamp: u64, event: Event) {
        let seq = self.next_sequence_number;
        self.next_sequence_number += 1;
        self.queue.push(TimestampedEvent {
            timestamp,
            sequence_number: seq,
            event,
        });
    }

    pub(crate) fn next(&mut self) -> Option<(u64, Event)> {
        self.queue.pop().map(|te| (te.timestamp, te.event))
    }
}
