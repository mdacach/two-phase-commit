//! Event queue with deterministic ordering.
//!
//! Events are ordered by `(timestamp, sequence_number)`.  The sequence number
//! breaks ties when multiple events share a timestamp, ensuring FIFO order
//! among same-time insertions allowing easier testing and reproducible runs.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::types::{ActorId, Message};

/// An event injected into the simulation from outside the protocol.
///
/// Crucially, this event is not subject to network faults or delays, as that
/// would only add debugging noise.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExternalEvent {
    /// Start a transaction in the 2PC protocol.
    ///
    /// This event happens exactly once, at the start of each simulation, and
    /// triggers the coordinator to start its messaging.
    StartTransaction,
    /// Call `tick` on a single actor at the current simulator time.
    Tick { to: ActorId },
    /// Call `tick` on every actor at the current simulator time.
    TickAll,
    /// Crash an actor.
    ///
    /// Messages delivered to crashed actors are dropped. The actor is
    /// non-operational until recovered.
    Crash(ActorId),
    /// Recover an actor.
    ///
    /// Triggers the [`recover`](crate::state_machine::StateMachine::recover) method,
    /// which recovers state based on durable storage.
    Recover(ActorId),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum InternalEvent {
    Deliver { to: ActorId, msg: Message },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Event {
    External(ExternalEvent),
    Internal(InternalEvent),
}

/// An event tagged with its delivery time and insertion order.
///
/// `Eq`/`Ord` compare all three fields (timestamp, sequence number, and event
/// payload) so that the ordering is fully deterministic even if two events
/// were hypothetically assigned the same sequence number.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TimestampedEvent {
    timestamp: u64,
    sequence_number: u64,
    event: Event,
}

/// Natural ordering is `(timestamp, sequence_number, event)`, reversed so
/// that `BinaryHeap` (a max-heap) pops the *smallest* timestamp first,
/// giving us min-heap semantics.
impl Ord for TimestampedEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        self.timestamp
            .cmp(&other.timestamp)
            .then_with(|| self.sequence_number.cmp(&other.sequence_number))
            .then_with(|| self.event.cmp(&other.event))
            .reverse()
    }
}

impl PartialOrd for TimestampedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(crate) struct EventQueue {
    queue: BinaryHeap<TimestampedEvent>,
    /// Monotonically increasing counter assigned to each inserted event.
    /// Breaks ties between events at the same timestamp, preserving FIFO
    /// insertion order and ensuring fully deterministic simulation runs.
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

    pub(crate) fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub(crate) fn next(&mut self) -> Option<(u64, Event)> {
        self.queue.pop().map(|te| (te.timestamp, te.event))
    }
}
