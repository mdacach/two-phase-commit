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
//! The coordinator retransmits on `tick` if `retransmit_timeout` time has
//! elapsed since the last send:
//! - In `Voting`: retransmit Prepare to nodes with no recorded vote.
//! - In `AwaitingAcks`: retransmit Decision to unacked nodes.
//!
//! # Crash recovery (WAL)
//!
//! The coordinator holds a `Wal` struct representing durable on-disk state.
//! On `recover()`:
//! - If `wal.decision` is `Some(d)`: reset to `Decided(d)`, retransmit on
//!   next tick.
//! - If `None`: reset to `Voting`, clear votes, retransmit Prepare on next
//!   tick.
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

use crate::state_machine::StateMachine;
use crate::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinatorPhase {
    Waiting,
    Voting,
    Decided(Decision),
    AwaitingAcks(Decision),
    Done(Decision),
}

struct Config {
    rng: ChaCha8Rng,
    abort_bias: f64,
    retransmit_timeout: u64,
}

struct Wal {
    decision: Option<Decision>,
}

pub struct Coordinator {
    nodes: Vec<NodeId>,
    phase: CoordinatorPhase,
    votes: BTreeMap<NodeId, Decision>,
    acks: BTreeSet<NodeId>,
    last_prepare_time: Option<u64>,
    last_decision_time: Option<u64>,
    wal: Wal,
    config: Config,
}

impl Coordinator {
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
            last_prepare_time: None,
            last_decision_time: None,
            wal: Wal { decision: None },
            config: Config {
                rng: ChaCha8Rng::seed_from_u64(rng_seed),
                abort_bias,
                retransmit_timeout,
            },
        }
    }

    pub fn phase(&self) -> CoordinatorPhase {
        self.phase
    }

    pub fn decision(&self) -> Option<Decision> {
        match self.phase {
            CoordinatorPhase::Decided(d)
            | CoordinatorPhase::AwaitingAcks(d)
            | CoordinatorPhase::Done(d) => Some(d),
            _ => None,
        }
    }

    pub fn votes(&self) -> &BTreeMap<NodeId, Decision> {
        &self.votes
    }

    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// Transition to `Decided` if a decision can be made:
    /// - Any Abort vote → `Decided(Abort)` immediately.
    /// - All votes in, all Commit → `Decided(Commit)` (subject to `abort_bias`).
    fn try_decide(&mut self) {
        if !matches!(self.phase, CoordinatorPhase::Voting) {
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
        self.wal.decision = Some(decision);
        self.phase = CoordinatorPhase::AwaitingAcks(decision);
        self.last_decision_time = Some(at_time);
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
                self.phase = CoordinatorPhase::Voting;
                self.last_prepare_time = Some(at_time);
                outgoing.extend(self.nodes.iter().map(|&node| Message {
                    message_type: MessageType::Prepare,
                    from: ActorId::Coordinator,
                    to: ActorId::Node(node),
                }));
            }
            (MessageType::VoteCommit | MessageType::VoteAbort, CoordinatorPhase::Voting) => {
                let node = match msg.from {
                    ActorId::Node(id) => id,
                    _ => {
                        eprintln!("[Coordinator] Ignoring vote from non-node {:?}", msg.from);
                        return outgoing;
                    }
                };
                if self.votes.contains_key(&node) {
                    eprintln!("[Coordinator] Ignoring duplicate vote from {node}");
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
            (MessageType::Ack, CoordinatorPhase::AwaitingAcks(d)) => {
                let node = match msg.from {
                    ActorId::Node(id) => id,
                    _ => return outgoing,
                };
                self.acks.insert(node);
                if self.acks.len() == self.nodes.len() {
                    self.phase = CoordinatorPhase::Done(d);
                }
            }
            (MessageType::Ack, CoordinatorPhase::Done(_)) => {
                // Duplicate ack after protocol complete, ignore.
            }
            (msg_type, phase) => {
                eprintln!("[Coordinator] Ignoring {msg_type:?} in {phase:?}");
            }
        }

        outgoing
    }

    fn tick(&mut self, at_time: u64) -> Vec<Message> {
        match self.phase {
            CoordinatorPhase::Voting => {
                let prob = (self.config.abort_bias / 10.0).clamp(0.0, 1.0);
                if prob > 0.0 && self.config.rng.random_bool(prob) {
                    self.phase = CoordinatorPhase::Decided(Decision::Abort);
                    return self.try_send_decision(at_time);
                }
                // Retransmit Prepare to nodes with no recorded vote.
                if let Some(last_time) = self.last_prepare_time {
                    if at_time.saturating_sub(last_time) >= self.config.retransmit_timeout {
                        self.last_prepare_time = Some(at_time);
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
                }
                vec![]
            }
            CoordinatorPhase::Decided(_) => self.try_send_decision(at_time),
            CoordinatorPhase::AwaitingAcks(decision) => {
                // Retransmit Decision to unacked nodes.
                if let Some(last_time) = self.last_decision_time {
                    if at_time.saturating_sub(last_time) >= self.config.retransmit_timeout {
                        self.last_decision_time = Some(at_time);
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
                }
                vec![]
            }
            _ => vec![],
        }
    }

    fn is_quiescent(&self) -> bool {
        matches!(
            self.phase,
            CoordinatorPhase::Done(_) | CoordinatorPhase::Waiting
        )
    }

    fn recover(&mut self, at_time: u64) {
        match self.wal.decision {
            Some(d) => {
                self.phase = CoordinatorPhase::Decided(d);
                self.acks.clear();
                self.last_decision_time =
                    Some(at_time.saturating_sub(self.config.retransmit_timeout));
            }
            None => {
                self.phase = CoordinatorPhase::Voting;
                self.votes.clear();
                self.acks.clear();
                self.last_prepare_time =
                    Some(at_time.saturating_sub(self.config.retransmit_timeout));
            }
        }
    }
}

#[cfg(test)]
mod tests;
