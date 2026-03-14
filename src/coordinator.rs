//! The 2PC coordinator state machine.
//!
//! # Phase transitions
//!
//! ```text
//! Waiting ──StartTransaction──▶ Voting ──try_decide──▶ Decided(d) ──try_send_decision──▶ AwaitingAcks(d) ──all acks──▶ Done(d)
//!                                  │                       ▲
//!                                  └──spontaneous abort────┘
//!                                       (tick, prob = abort_bias / 10)
//! ```
//!
//! # Retransmission
//!
//! Reliable communication is accomplished by message retransmission:
//! The coordinator retransmits on [`tick`](StateMachine::tick) if
//! `retransmit_timeout` time has elapsed since the last send:
//! - In [`Voting`](CoordinatorPhase::Voting): retransmit
//!   [`Prepare`](MessageType::Prepare) to nodes with no recorded vote.
//! - In [`AwaitingAcks`](CoordinatorPhase::AwaitingAcks): retransmit
//!   the decision to unacked nodes.
//!
//! # Crash recovery
//!
//! The coordinator holds a [`DurableState`] struct representing on-disk state
//! that survives crashes. On [`recover`](StateMachine::recover):
//! - If `durable_state.decision` is `Some(d)`: reset to
//!   [`Decided(d)`](CoordinatorPhase::Decided), retransmit on next tick.
//! - If `None`: reset to [`Voting`](CoordinatorPhase::Voting), clear votes,
//!   retransmit [`Prepare`](MessageType::Prepare) on next tick.
//!
//! # `abort_bias`
//!
//! Controls two distinct behaviours (neither has a TLA+ counterpart):
//!
//! 1. **Vote-triggered abort** — When all participants vote Commit, the
//!    coordinator still aborts with probability `abort_bias`. This models an
//!    additional policy check or crash after vote collection.
//! 2. **Spontaneous abort** — On every `tick` while in `Voting`, the
//!    coordinator aborts with probability `abort_bias / 10`. This models a
//!    coordinator timeout that fires independently of vote arrival.
//!
//! With `abort_bias = 0.0` the coordinator is deterministic: it commits iff
//! every participant votes Commit. This is the behaviour the TLA+ spec models.
//!
//! # `on_message` calls `tick` internally
//!
//! Every `on_message` call begins with `self.tick(at_time)`, so the spontaneous
//! abort check runs before each message is processed. This means a vote delivery
//! can be pre-empted by a spontaneous abort that fires in the leading tick. The
//! simulator relies on this: it does **not** tick actors before delivering
//! messages.

use std::collections::{BTreeMap, BTreeSet};

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use tracing::warn;

use crate::state_machine::StateMachine;
use crate::types::*;

/// Coordinator protocol phase.
///
/// - `Waiting` — idle, no transaction in progress.
/// - `Voting` — Prepare sent; collecting votes from participants.
/// - `Decided` — decision made but not yet broadcast.
/// - `AwaitingAcks` — decision broadcast; waiting for participant Acks.
/// - `Done` — all Acks received; protocol complete.
///
/// `Voting` and `AwaitingAcks` carry the timestamp of the last send
/// (`last_prepare_time` / `last_decision_time`), used to gate retransmission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinatorPhase {
    Waiting,
    Voting {
        last_prepare_time: u64,
    },
    Decided(Decision),
    AwaitingAcks {
        decision: Decision,
        last_decision_time: u64,
    },
    Done(Decision),
}

/// Simulation configuration (not part of protocol state).
struct Config {
    rng: ChaCha8Rng,
    abort_bias: f64,
    retransmit_timeout: u64,
}

/// Durable state that survives crashes. Written before sending messages,
/// read on recovery. Volatile state (votes, acks) is intentionally excluded.
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
    nodes: Vec<NodeId>,
    phase: CoordinatorPhase,
    votes: BTreeMap<NodeId, Decision>,
    acks: BTreeSet<NodeId>,
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
            votes: BTreeMap::new(),
            acks: BTreeSet::new(),
            durable_state: DurableState { decision: None },
            config: Config {
                rng: ChaCha8Rng::seed_from_u64(rng_seed),
                abort_bias,
                retransmit_timeout,
            },
        }
    }

    /// Current protocol phase.
    pub fn phase(&self) -> CoordinatorPhase {
        self.phase
    }

    /// The decision this coordinator reached, if any.
    pub fn decision(&self) -> Option<Decision> {
        match self.phase {
            CoordinatorPhase::Decided(d) | CoordinatorPhase::Done(d) => Some(d),
            CoordinatorPhase::AwaitingAcks { decision, .. } => Some(decision),
            _ => None,
        }
    }

    /// Votes received so far (node → vote). Cleared on crash recovery without
    /// a WAL decision.
    pub fn votes(&self) -> &BTreeMap<NodeId, Decision> {
        &self.votes
    }

    /// The set of participant node IDs this coordinator manages.
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// Transition to `Decided` if a decision can be made:
    /// - Any Abort vote → `Decided(Abort)` immediately.
    /// - All votes in, all Commit → `Decided(Commit)` (subject to `abort_bias`).
    fn try_decide(&mut self) {
        if !matches!(self.phase, CoordinatorPhase::Voting { .. }) {
            return;
        }

        if self.votes.values().any(|&v| v == Decision::Abort) {
            self.phase = CoordinatorPhase::Decided(Decision::Abort);
        } else if self.votes.len() == self.nodes.len() {
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

    /// If in `Decided`, write to WAL, broadcast the decision to all
    /// participants, and transition to `AwaitingAcks`. No-op in any other phase.
    fn try_send_decision(&mut self, at_time: u64) -> Vec<Message> {
        let CoordinatorPhase::Decided(decision) = self.phase else {
            return vec![];
        };
        self.durable_state.decision = Some(decision);
        self.phase = CoordinatorPhase::AwaitingAcks {
            decision,
            last_decision_time: at_time,
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

        match (msg.message_type, self.phase) {
            (MessageType::StartTransaction, CoordinatorPhase::Waiting) => {
                self.phase = CoordinatorPhase::Voting {
                    last_prepare_time: at_time,
                };
                outgoing.extend(self.nodes.iter().map(|&node| Message {
                    message_type: MessageType::Prepare,
                    from: ActorId::Coordinator,
                    to: ActorId::Node(node),
                }));
            }
            (MessageType::VoteCommit | MessageType::VoteAbort, CoordinatorPhase::Voting { .. }) => {
                let node = match msg.from {
                    ActorId::Node(id) => id,
                    _ => {
                        warn!(from = ?msg.from, "Ignoring vote from non-node");
                        return outgoing;
                    }
                };
                if self.votes.contains_key(&node) {
                    warn!(%node, "Ignoring duplicate vote");
                    return outgoing;
                }
                let vote = if msg.message_type == MessageType::VoteCommit {
                    Decision::Commit
                } else {
                    Decision::Abort
                };
                self.votes.insert(node, vote);
                self.try_decide();
                outgoing.extend(self.try_send_decision(at_time));
            }
            (MessageType::Ack, CoordinatorPhase::AwaitingAcks { decision, .. }) => {
                let node = match msg.from {
                    ActorId::Node(id) => id,
                    _ => return outgoing,
                };
                self.acks.insert(node);
                if self.acks.len() == self.nodes.len() {
                    self.phase = CoordinatorPhase::Done(decision);
                }
            }
            (MessageType::Ack, CoordinatorPhase::Done(_)) => {
                // Duplicate ack after protocol complete, ignore.
            }
            (msg_type, phase) => {
                warn!(?msg_type, ?phase, "Ignoring unexpected message");
            }
        }

        outgoing
    }

    fn tick(&mut self, at_time: u64) -> Vec<Message> {
        match self.phase {
            CoordinatorPhase::Voting { last_prepare_time } => {
                let prob = (self.config.abort_bias / 10.0).clamp(0.0, 1.0);
                if self.config.rng.random_bool(prob) {
                    self.phase = CoordinatorPhase::Decided(Decision::Abort);
                    return self.try_send_decision(at_time);
                }
                // Retransmit Prepare to nodes with no recorded vote.
                if at_time.saturating_sub(last_prepare_time) >= self.config.retransmit_timeout {
                    self.phase = CoordinatorPhase::Voting {
                        last_prepare_time: at_time,
                    };
                    return self
                        .nodes
                        .iter()
                        .filter(|n| !self.votes.contains_key(n))
                        .map(|&node| Message {
                            message_type: MessageType::Prepare,
                            from: ActorId::Coordinator,
                            to: ActorId::Node(node),
                        })
                        .collect();
                }
                vec![]
            }
            CoordinatorPhase::Decided(_) => self.try_send_decision(at_time),
            CoordinatorPhase::AwaitingAcks {
                decision,
                last_decision_time,
            } => {
                // Retransmit Decision to unacked nodes.
                if at_time.saturating_sub(last_decision_time) >= self.config.retransmit_timeout {
                    self.phase = CoordinatorPhase::AwaitingAcks {
                        decision,
                        last_decision_time: at_time,
                    };
                    let message_type = match decision {
                        Decision::Commit => MessageType::DecisionCommit,
                        Decision::Abort => MessageType::DecisionAbort,
                    };
                    return self
                        .nodes
                        .iter()
                        .filter(|n| !self.acks.contains(n))
                        .map(|&node| Message {
                            message_type,
                            from: ActorId::Coordinator,
                            to: ActorId::Node(node),
                        })
                        .collect();
                }
                vec![]
            }
            _ => vec![],
        }
    }

    /// Quiescent in `Waiting` (protocol not started) or `Done` (protocol
    /// complete). All other phases may produce messages on tick.
    fn is_quiescent(&self) -> bool {
        matches!(
            self.phase,
            CoordinatorPhase::Done(_) | CoordinatorPhase::Waiting
        )
    }

    /// Restore volatile state from durable storage after a crash.
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
                self.acks.clear();
            }
            None => {
                self.phase = CoordinatorPhase::Voting {
                    last_prepare_time: at_time.saturating_sub(self.config.retransmit_timeout),
                };
                self.votes.clear();
                self.acks.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests;
