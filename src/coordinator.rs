//! The 2PC coordinator state machine.
//!
//! # Phase transitions
//!
//! ```text
//! Waiting ──StartTransaction──▶ Voting ──try_decide──▶ Decided(d) ──try_send_decision──▶ Done(d)
//!                                  │                       ▲
//!                                  └──spontaneous abort────┘
//!                                       (tick, prob = abort_bias / 10)
//! ```
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

use std::collections::BTreeMap;

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
    Done(Decision),
}

struct Config {
    rng: ChaCha8Rng,
    abort_bias: f64,
}

// REVIEW: Derive debug for coordinator to be used in logs and errors if needed.
pub struct Coordinator {
    nodes: Vec<NodeId>,
    phase: CoordinatorPhase,
    votes: BTreeMap<NodeId, Decision>,
    config: Config,
}

impl Coordinator {
    pub fn new(nodes: Vec<NodeId>, rng_seed: u64, abort_bias: f64) -> Self {
        Self {
            nodes,
            phase: CoordinatorPhase::Waiting,
            votes: BTreeMap::new(),
            config: Config {
                rng: ChaCha8Rng::seed_from_u64(rng_seed),
                abort_bias,
            },
        }
    }

    pub fn phase(&self) -> CoordinatorPhase {
        self.phase
    }

    pub fn decision(&self) -> Option<Decision> {
        match self.phase {
            CoordinatorPhase::Decided(d) | CoordinatorPhase::Done(d) => Some(d),
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

    /// If in `Decided`, broadcast the decision to all participants and
    /// transition to `Done`. No-op in any other phase.
    fn try_send_decision(&mut self) -> Vec<Message> {
        let CoordinatorPhase::Decided(decision) = self.phase else {
            return vec![];
        };
        self.phase = CoordinatorPhase::Done(decision);
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

// REVIEW: Use tracing for logging.
impl StateMachine for Coordinator {
    fn on_message(&mut self, msg: &Message, at_time: u64) -> Vec<Message> {
        let mut outgoing = self.tick(at_time);

        match (msg.message_type, self.phase) {
            (MessageType::StartTransaction, CoordinatorPhase::Waiting) => {
                self.phase = CoordinatorPhase::Voting;
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
                outgoing.extend(self.try_send_decision());
            }
            (msg_type, phase) => {
                eprintln!("[Coordinator] Ignoring {msg_type:?} in {phase:?}");
            }
        }

        outgoing
    }

    fn tick(&mut self, _at_time: u64) -> Vec<Message> {
        if let CoordinatorPhase::Voting = self.phase {
            let prob = (self.config.abort_bias / 10.0).clamp(0.0, 1.0);
            if prob > 0.0 && self.config.rng.random_bool(prob) {
                self.phase = CoordinatorPhase::Decided(Decision::Abort);
            }
        }
        self.try_send_decision()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_nodes() -> Vec<NodeId> {
        vec![NodeId(0), NodeId(1)]
    }

    #[test]
    fn start_transaction_sends_prepare() {
        let mut coord = Coordinator::new(two_nodes(), 0, 0.0);
        let start = Message {
            message_type: MessageType::StartTransaction,
            from: ActorId::Coordinator,
            to: ActorId::Coordinator,
        };
        let msgs = coord.on_message(&start, 0);
        assert_eq!(coord.phase(), CoordinatorPhase::Voting);
        assert_eq!(msgs.len(), 2);
        assert!(msgs.iter().all(|m| m.message_type == MessageType::Prepare));
    }

    #[test]
    fn all_commit_votes_without_abort_bias() {
        let mut coord = Coordinator::new(two_nodes(), 0, 0.0);
        coord.phase = CoordinatorPhase::Voting;

        let vote0 = Message {
            message_type: MessageType::VoteCommit,
            from: ActorId::Node(NodeId(0)),
            to: ActorId::Coordinator,
        };
        coord.on_message(&vote0, 1);
        assert_eq!(coord.phase(), CoordinatorPhase::Voting);

        let vote1 = Message {
            message_type: MessageType::VoteCommit,
            from: ActorId::Node(NodeId(1)),
            to: ActorId::Coordinator,
        };
        let msgs = coord.on_message(&vote1, 2);
        assert_eq!(coord.phase(), CoordinatorPhase::Done(Decision::Commit));
        assert_eq!(coord.decision(), Some(Decision::Commit));
        assert_eq!(msgs.len(), 2);
        assert!(
            msgs.iter()
                .all(|m| m.message_type == MessageType::DecisionCommit)
        );
    }

    #[test]
    fn abort_vote_decides_abort() {
        let mut coord = Coordinator::new(two_nodes(), 0, 0.0);
        coord.phase = CoordinatorPhase::Voting;

        let vote = Message {
            message_type: MessageType::VoteAbort,
            from: ActorId::Node(NodeId(0)),
            to: ActorId::Coordinator,
        };
        let msgs = coord.on_message(&vote, 1);
        assert_eq!(coord.phase(), CoordinatorPhase::Done(Decision::Abort));
        assert_eq!(coord.decision(), Some(Decision::Abort));
        assert_eq!(msgs.len(), 2);
        assert!(
            msgs.iter()
                .all(|m| m.message_type == MessageType::DecisionAbort)
        );
    }

    #[test]
    fn tick_decided_sends_decision_messages() {
        let mut coord = Coordinator::new(two_nodes(), 0, 0.0);
        coord.phase = CoordinatorPhase::Decided(Decision::Commit);

        let msgs = coord.tick(0);
        assert_eq!(coord.phase(), CoordinatorPhase::Done(Decision::Commit));
        assert_eq!(msgs.len(), 2);
        assert!(
            msgs.iter()
                .all(|m| m.message_type == MessageType::DecisionCommit)
        );
    }
}
