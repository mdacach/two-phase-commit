//! The 2PC coordinator state machine.
//!
//! # Phase transitions
//!
//! ```text
//!                       ┌─────────┐
//!                       │ Waiting │
//!                       └────┬────┘
//!                            │ StartTransaction
//!                            v
//!                       ┌─────────┐
//!               ┌───────│ Voting  │───────┐
//!               │       └─────────┘       │
//!         all votes in               tick timeout
//!               │                (spontaneous abort)
//!               v                         v
//!                      ┌───────────┐
//!                      │Decided(d) │
//!                      └─────┬─────┘
//!                            │ broadcast
//!                            v
//!                    ┌───────────────┐
//!                    │AwaitingAcks(d)│
//!                    └───────┬───────┘
//!                            │ all acks
//!                            v
//!                       ┌─────────┐
//!                       │ Done(d) │
//!                       └─────────┘
//! ```
//!
//! # Retransmission
//!
//! Reliable communication is accomplished through message retransmission:
//! The coordinator retransmits on [`tick`](StateMachine::tick) if
//! `retransmit_timeout` time has elapsed since the last send:
//! - In [`Voting`](CoordinatorPhase::Voting): retransmit
//!   [`Prepare`](MessageType::Prepare) to nodes with no recorded vote.
//! - In [`AwaitingAcks`](CoordinatorPhase::AwaitingAcks): retransmit
//!   the decision to unacked nodes.
//!
//! # Crash recovery
//!
//! The coordinator persists its decision to [`DurableState`] before
//! broadcasting. On [`recover`](StateMachine::recover):
//! - Durable decision → reset to [`Decided`](CoordinatorPhase::Decided),
//!   re-broadcast decision on next tick.
//! - No durable decision → reset to [`Voting`](CoordinatorPhase::Voting)
//!   and retrigger a voting round.
//!
//! # `abort_bias`
//!
//! Controls two distinct behaviours:
//!
//! 1. **Vote-triggered abort** — When all participants vote Commit, the
//!    coordinator still aborts with probability `abort_bias`. This models an
//!    additional policy check or failure after vote collection.
//! 2. **Spontaneous abort** — On every `tick` while in `Voting`, the
//!    coordinator aborts with probability `abort_bias / 10`.
//!
//! With `abort_bias = 0.0` the coordinator is deterministic: it commits if
//! and only if every participant votes Commit.

use std::collections::{BTreeMap, BTreeSet};

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use tracing::{trace, warn};

use crate::state_machine::StateMachine;
use crate::types::*;

/// Coordinator protocol phase.
///
/// - `Waiting` — idle, no transaction in progress.
/// - `Voting` — Prepare sent; collecting votes from participants.
///   Holds the accumulated votes and the timestamp of the last Prepare send.
/// - `Decided` — decision made but not yet broadcast.
/// - `AwaitingAcks` — decision broadcast; waiting for participant Acks.
///   Holds the pending ack set and the timestamp of the last Decision send.
/// - `Done` — all Acks received; protocol complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinatorPhase {
    Waiting,
    Voting {
        last_prepare_time: u64,
        votes: BTreeMap<NodeId, Vote>,
    },
    Decided(Decision),
    AwaitingAcks {
        decision: Decision,
        last_decision_time: u64,
        acks: BTreeSet<NodeId>,
    },
    Done(Decision),
}

/// Simulation configuration.
struct Config {
    rng: ChaCha8Rng,
    abort_bias: f64,
    retransmit_timeout: u64,
}

/// Durable state that survives crashes. Written before sending messages,
/// read on recovery.
struct DurableState {
    decision: Option<Decision>,
}

/// The coordinator of the two-phase commit protocol.
///
/// The coordinator drives the protocol: it broadcasts Prepare, collects
/// votes, decides, broadcasts the decision, and collects Acks. See the
/// [module docs](self) for the full phase-transition diagram and
/// crash-recovery semantics.
pub struct Coordinator {
    /// Participant node IDs managed by this coordinator.
    nodes: Vec<NodeId>,
    phase: CoordinatorPhase,
    durable_state: DurableState,
    config: Config,
}

impl Coordinator {
    /// Create a coordinator managing the given participant `nodes`.
    ///
    /// - `rng_seed` — deterministic seed for the abort-bias coin flips.
    /// - `abort_bias` — probability of aborting despite unanimous commit (see [module docs](self)).
    /// - `retransmit_timeout` — ticks before retransmitting Prepare or Decision.
    pub fn new(
        nodes: Vec<NodeId>,
        rng_seed: u64,
        abort_bias: f64,
        retransmit_timeout: u64,
    ) -> Self {
        Self {
            nodes,
            phase: CoordinatorPhase::Waiting,
            durable_state: DurableState { decision: None },
            config: Config {
                rng: ChaCha8Rng::seed_from_u64(rng_seed),
                abort_bias,
                retransmit_timeout,
            },
        }
    }

    /// Current protocol phase.
    pub fn phase(&self) -> &CoordinatorPhase {
        &self.phase
    }

    /// The decision this coordinator reached, if any.
    pub fn decision(&self) -> Option<Decision> {
        match &self.phase {
            CoordinatorPhase::Decided(d) | CoordinatorPhase::Done(d) => Some(*d),
            CoordinatorPhase::AwaitingAcks { decision, .. } => Some(*decision),
            _ => None,
        }
    }

    /// The set of participant node IDs this coordinator manages.
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// Transition to `Decided` if a decision can be made:
    /// - Any Abort vote → `Decided(Abort)` immediately.
    /// - All votes in, all Commit → `Decided(Commit)` (subject to `abort_bias`
    /// coin flip).
    fn try_decide(&mut self) {
        let CoordinatorPhase::Voting { ref votes, .. } = self.phase else {
            return;
        };
        let has_abort = votes.values().any(|&v| v == Vote::Abort);
        let all_in = votes.len() == self.nodes.len();

        if has_abort {
            self.phase = CoordinatorPhase::Decided(Decision::Abort);
        } else if all_in {
            let decision = if self
                .config
                .rng
                .random_bool(self.config.abort_bias.clamp(0.0, 1.0))
            {
                Decision::Abort
            } else {
                Decision::Commit
            };
            self.phase = CoordinatorPhase::Decided(decision);
        }
    }

    /// If in `Decided`, write to durable storage, broadcast the decision to all
    /// participants, and transition to `AwaitingAcks`. No-op in any other phase.
    fn try_send_decision(&mut self, at_time: u64) -> Vec<Message> {
        let CoordinatorPhase::Decided(decision) = self.phase else {
            return vec![];
        };
        self.durable_state.decision = Some(decision);
        self.phase = CoordinatorPhase::AwaitingAcks {
            decision,
            last_decision_time: at_time,
            acks: BTreeSet::new(),
        };
        let message_type = match decision {
            Decision::Commit => MessageType::DecisionCommit,
            Decision::Abort => MessageType::DecisionAbort,
        };
        self.nodes
            .iter()
            .map(|&node| Message {
                message_type,
                from: ActorId::Coordinator,
                to: ActorId::Node(node),
            })
            .collect()
    }
}

impl StateMachine for Coordinator {
    fn on_message(&mut self, msg: &Message, at_time: u64) -> Vec<Message> {
        let mut outgoing = self.tick(at_time);

        match msg.message_type {
            MessageType::StartTransaction if matches!(self.phase, CoordinatorPhase::Waiting) => {
                self.phase = CoordinatorPhase::Voting {
                    last_prepare_time: at_time,
                    votes: BTreeMap::new(),
                };
                outgoing.extend(self.nodes.iter().map(|&node| Message {
                    message_type: MessageType::Prepare,
                    from: ActorId::Coordinator,
                    to: ActorId::Node(node),
                }));
            }
            MessageType::VoteCommit | MessageType::VoteAbort
                if matches!(self.phase, CoordinatorPhase::Voting { .. }) =>
            {
                let node = match msg.from {
                    ActorId::Node(id) => id,
                    _ => {
                        warn!(from = ?msg.from, "Ignoring vote from non-node");
                        return outgoing;
                    }
                };
                let vote = if msg.message_type == MessageType::VoteCommit {
                    Vote::Commit
                } else {
                    Vote::Abort
                };
                if let CoordinatorPhase::Voting { ref mut votes, .. } = self.phase {
                    if votes.contains_key(&node) {
                        warn!(%node, "Ignoring duplicate vote");
                        return outgoing;
                    }
                    votes.insert(node, vote);
                }
                self.try_decide();
                outgoing.extend(self.try_send_decision(at_time));
            }
            MessageType::Ack if matches!(self.phase, CoordinatorPhase::AwaitingAcks { .. }) => {
                let node = match msg.from {
                    ActorId::Node(id) => id,
                    _ => return outgoing,
                };
                let done = match &mut self.phase {
                    CoordinatorPhase::AwaitingAcks { acks, decision, .. } => {
                        acks.insert(node);
                        if acks.len() == self.nodes.len() {
                            Some(*decision)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(decision) = done {
                    self.phase = CoordinatorPhase::Done(decision);
                }
            }
            MessageType::Ack if matches!(self.phase, CoordinatorPhase::Done(_)) => {
                trace!("Duplicate ack after protocol complete, ignoring");
            }
            _ => {
                warn!(msg_type = ?msg.message_type, phase = ?self.phase, "Ignoring unexpected message");
            }
        }

        outgoing
    }

    fn tick(&mut self, at_time: u64) -> Vec<Message> {
        // Spontaneous abort check (doesn't need votes).
        if matches!(self.phase, CoordinatorPhase::Voting { .. }) {
            let prob = (self.config.abort_bias / 10.0).clamp(0.0, 1.0);
            if self.config.rng.random_bool(prob) {
                self.phase = CoordinatorPhase::Decided(Decision::Abort);
                return self.try_send_decision(at_time);
            }
        }

        // Voting: retransmit Prepare to nodes with no recorded vote.
        if let CoordinatorPhase::Voting {
            ref mut last_prepare_time,
            ref votes,
        } = self.phase
        {
            if at_time.saturating_sub(*last_prepare_time) >= self.config.retransmit_timeout {
                *last_prepare_time = at_time;
                return self
                    .nodes
                    .iter()
                    .filter(|n| !votes.contains_key(n))
                    .map(|&node| Message {
                        message_type: MessageType::Prepare,
                        from: ActorId::Coordinator,
                        to: ActorId::Node(node),
                    })
                    .collect();
            }
            return vec![];
        }

        // Decided: send decision.
        if matches!(self.phase, CoordinatorPhase::Decided(_)) {
            return self.try_send_decision(at_time);
        }

        // AwaitingAcks: retransmit Decision to unacked nodes.
        if let CoordinatorPhase::AwaitingAcks {
            decision,
            ref mut last_decision_time,
            ref acks,
        } = self.phase
        {
            if at_time.saturating_sub(*last_decision_time) >= self.config.retransmit_timeout {
                *last_decision_time = at_time;
                let message_type = match decision {
                    Decision::Commit => MessageType::DecisionCommit,
                    Decision::Abort => MessageType::DecisionAbort,
                };
                return self
                    .nodes
                    .iter()
                    .filter(|n| !acks.contains(n))
                    .map(|&node| Message {
                        message_type,
                        from: ActorId::Coordinator,
                        to: ActorId::Node(node),
                    })
                    .collect();
            }
            return vec![];
        }

        vec![]
    }

    /// Quiescent in `Waiting` (protocol not started) or `Done` (protocol
    /// complete). All other phases may produce messages on tick.
    fn is_quiescent(&self) -> bool {
        matches!(
            self.phase,
            CoordinatorPhase::Done(_) | CoordinatorPhase::Waiting
        )
    }

    /// Restore state from durable storage after a crash.
    ///
    /// - If durable state contains a decision, reset to `Decided(d)` so the
    ///   next tick broadcasts the decision and transitions to `AwaitingAcks`.
    /// - If durable state is empty, reset to `Voting` with cleared votes.
    ///
    /// In both cases the retransmit timestamp is backdated by
    /// `retransmit_timeout` so that the very next tick triggers an immediate
    /// retransmission (the elapsed time equals the timeout).
    fn recover(&mut self, at_time: u64) {
        match self.durable_state.decision {
            Some(d) => {
                self.phase = CoordinatorPhase::Decided(d);
            }
            None => {
                self.phase = CoordinatorPhase::Voting {
                    last_prepare_time: at_time.saturating_sub(self.config.retransmit_timeout),
                    votes: BTreeMap::new(),
                };
            }
        }
    }
}

#[cfg(test)]
mod tests;
