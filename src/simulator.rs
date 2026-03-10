//! Discrete-event simulator for the 2PC protocol.
//!
//! # Execution model
//!
//! Events live in a priority queue ordered by `(timestamp, sequence_number)`.
//! On each [`step`](Simulator::step):
//!
//! 1. The earliest event is popped.
//! 2. The simulator's clock advances to its timestamp.
//! 3. The event is dispatched to the appropriate actor.
//! 4. Any messages returned by the actor are enqueued as `Deliver` events
//!    at `clock + 1 + random_delay`, where the delay is drawn from
//!    `delivery_delay`.
//! 5. Safety invariants are checked (panics on violation).
//!
//! ## Quiescence
//!
//! After all external events have been processed, [`drain`](Simulator::drain)
//! probes for quiescence by injecting `TickAll` events and checking whether any
//! actor produces new messages.  The protocol is quiescent when the event queue
//! is empty and a full `TickAll` round produces nothing.

mod event;

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::coordinator::Coordinator;
use crate::participant::Participant;
use crate::properties;
use crate::state_machine::StateMachine;
use crate::types::*;

pub use event::ExternalEvent;
use event::{Event, InternalEvent};

// REVIEW: Make it so the delivery delay range contains 1, and instead messages arrive at clock + delivery_delay.
//         Will need to update documentation too.
pub struct Simulator {
    coordinator: Coordinator,
    participants: BTreeMap<NodeId, Participant>,
    event_queue: event::EventQueue,
    clock: u64,
    rng: ChaCha8Rng,
    /// Random delay added to each message delivery. An empty range means
    /// zero delay (messages arrive at `clock + 1`).
    delivery_delay: Range<u64>,
    /// Append-only record of every event processed and message sent.
    action_log: Vec<LogEntry>,
}

impl Simulator {
    pub fn new(n_participants: u8, seed: u64, abort_bias: f64, delivery_delay: Range<u64>) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);

        let nodes: Vec<NodeId> = (0..n_participants).map(NodeId).collect();

        let seed: u64 = rng.random();
        let coordinator = Coordinator::new(nodes.clone(), seed, abort_bias);

        let mut participants = BTreeMap::new();
        for &node_id in &nodes {
            let seed: u64 = rng.random();
            participants.insert(node_id, Participant::new(node_id, seed, 0.2));
        }

        Self {
            coordinator,
            participants,
            event_queue: event::EventQueue::new(),
            clock: 0,
            rng,
            delivery_delay,
            action_log: Vec::new(),
        }
    }

    pub fn enqueue_external(&mut self, event: ExternalEvent, at_time: u64) {
        self.event_queue.insert(at_time, Event::External(event));
    }

    fn enqueue_outgoing(&mut self, messages: Vec<Message>) {
        for msg in messages {
            let delay = if !self.delivery_delay.is_empty() {
                self.rng.random_range(self.delivery_delay.clone())
            } else {
                0
            };
            let deliver_at = self.clock + 1 + delay;
            self.action_log.push(LogEntry::Send {
                at: self.clock,
                deliver_at,
                msg: msg.clone(),
            });
            self.event_queue.insert(
                deliver_at,
                Event::Internal(InternalEvent::Deliver { to: msg.to, msg }),
            );
        }
    }

    /// Process one event. Returns `false` if the queue was empty.
    ///
    /// Panics if a safety invariant is violated after the event.
    pub fn step(&mut self) -> bool {
        let Some((timestamp, ev)) = self.event_queue.next() else {
            return false;
        };

        self.clock = timestamp;

        let outgoing = match ev {
            Event::External(ref ext) => {
                self.action_log.push(LogEntry::ExternalEvent {
                    at: self.clock,
                    event: ext.clone(),
                });
                match ext {
                    ExternalEvent::StartTransaction => {
                        let msg = Message {
                            message_type: MessageType::StartTransaction,
                            from: ActorId::Coordinator,
                            to: ActorId::Coordinator,
                        };
                        self.coordinator.on_message(&msg, self.clock)
                    }
                    ExternalEvent::Tick { to } => match to {
                        ActorId::Coordinator => self.coordinator.tick(self.clock),
                        ActorId::Node(id) => self
                            .participants
                            .get_mut(id)
                            .map(|p| p.tick(self.clock))
                            .unwrap_or_default(),
                    },
                    ExternalEvent::TickAll => {
                        let mut out = self.coordinator.tick(self.clock);
                        let node_ids: Vec<NodeId> = self.participants.keys().copied().collect();
                        for id in node_ids {
                            if let Some(p) = self.participants.get_mut(&id) {
                                out.extend(p.tick(self.clock));
                            }
                        }
                        out
                    }
                }
            }
            Event::Internal(InternalEvent::Deliver { to, msg }) => {
                self.action_log.push(LogEntry::Deliver {
                    at: self.clock,
                    msg: msg.clone(),
                });
                match to {
                    ActorId::Coordinator => self.coordinator.on_message(&msg, self.clock),
                    ActorId::Node(id) => self
                        .participants
                        .get_mut(&id)
                        .map(|p| p.on_message(&msg, self.clock))
                        .unwrap_or_default(),
                }
            }
        };

        self.enqueue_outgoing(outgoing);

        if let Err(e) = properties::check_all_invariants(&self.coordinator, &self.participants) {
            panic!(
                "Invariant violation at clock={}: {e}\n  coordinator: phase={:?}, decision={:?}, votes={:?}",
                self.clock,
                self.coordinator.phase(),
                self.coordinator.decision(),
                self.coordinator.votes(),
            );
        }

        true
    }

    /// Process events until the queue is empty.
    pub fn run(&mut self) {
        while self.step() {}
    }

    /// Drain the event queue by ticking all actors until no new events are produced,
    /// up to `max_rounds` steps. Returns whether the system is quiescent.
    pub fn drain(&mut self, max_rounds: usize) -> bool {
        for _ in 0..max_rounds {
            if self.step() {
                continue;
            }
            // Queue is empty — inject a TickAll probe to check for quiescence.
            self.clock += 1;
            self.enqueue_external(ExternalEvent::TickAll, self.clock);
            self.step(); // processes TickAll with invariant checking
            // If nothing new was enqueued, we're quiescent.
            if !self.step() {
                return true;
            }
        }
        false
    }

    pub fn coordinator(&self) -> &Coordinator {
        &self.coordinator
    }

    pub fn participants(&self) -> &BTreeMap<NodeId, Participant> {
        &self.participants
    }

    pub fn log(&self) -> &[LogEntry] {
        &self.action_log
    }

    /// Format the full event log as a human-readable timeline.
    pub fn format_log(&self) -> String {
        self.action_log
            .iter()
            .map(|entry| entry.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A record of something that happened during simulation.
#[derive(Debug, Clone)]
pub enum LogEntry {
    /// An external event was processed.
    ExternalEvent { at: u64, event: ExternalEvent },
    /// A message was delivered to an actor and processed.
    Deliver { at: u64, msg: Message },
    /// A message was enqueued for future delivery.
    Send {
        at: u64,
        deliver_at: u64,
        msg: Message,
    },
}

impl fmt::Display for LogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogEntry::ExternalEvent { at, event } => {
                write!(f, "t={at:<4} [Event]   {event:?}")
            }
            LogEntry::Deliver { at, msg } => {
                write!(
                    f,
                    "t={at:<4} [Deliver] {} → {}: {:?}",
                    msg.from, msg.to, msg.message_type,
                )
            }
            LogEntry::Send {
                at,
                deliver_at,
                msg,
            } => {
                write!(
                    f,
                    "t={at:<4} [Send]    {} → {}: {:?} (deliver@{deliver_at})",
                    msg.from, msg.to, msg.message_type,
                )
            }
        }
    }
}
