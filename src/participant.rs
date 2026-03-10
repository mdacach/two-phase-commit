//! The 2PC participant state machine.
//!
//! # Phase transitions
//!
//! ```text
//! Waiting ──Prepare──▶ Voted(d)
//!    │                    │
//!    └──DecisionX──▶ Decided { vote: None, … }
//!                         │
//!         Voted(d) ──DecisionX──▶ Decided { vote: Some(d), … }
//! ```
//!
//! A participant can receive the coordinator's decision *before* the Prepare
//! message (if another participant's abort caused an early decision).  In that
//! case `vote` is `None` — the participant never voted.
//!
//! # `abort_bias`
//!
//! Controls the probability that the participant votes Abort when it receives
//! Prepare.  With `abort_bias = 0.0` the participant always votes Commit; with
//! `1.0` it always votes Abort.  [`with_fixed_vote`](Participant::with_fixed_vote)
//! is sugar for the extreme values.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::state_machine::StateMachine;
use crate::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipantPhase {
    Waiting,
    Voted(Decision),
    /// `vote` is `None` if the decision arrived before Prepare (the participant
    /// never voted). The Alloy spec permits this: `participantReceiveDecision`
    /// only requires `n not in participantDecided`, not that n has voted.
    Decided {
        vote: Option<Decision>,
        decision: Decision,
    },
}

struct Config {
    rng: ChaCha8Rng,
    abort_bias: f64,
}

pub struct Participant {
    id: NodeId,
    phase: ParticipantPhase,
    config: Config,
}

impl Participant {
    pub fn new(id: NodeId, rng_seed: u64, abort_bias: f64) -> Self {
        Self {
            id,
            phase: ParticipantPhase::Waiting,
            config: Config {
                rng: ChaCha8Rng::seed_from_u64(rng_seed),
                abort_bias,
            },
        }
    }

    /// Create a participant with a deterministic vote.
    /// `Commit` → `abort_bias = 0.0`, `Abort` → `abort_bias = 1.0`.
    pub fn with_fixed_vote(id: NodeId, vote: Decision) -> Self {
        let abort_bias = match vote {
            Decision::Commit => 0.0,
            Decision::Abort => 1.0,
        };
        Self::new(id, 0, abort_bias)
    }

    pub fn phase(&self) -> ParticipantPhase {
        self.phase
    }

    pub fn vote(&self) -> Option<Decision> {
        match self.phase {
            ParticipantPhase::Voted(v) => Some(v),
            ParticipantPhase::Decided { vote, .. } => vote,
            ParticipantPhase::Waiting => None,
        }
    }

    pub fn decision(&self) -> Option<Decision> {
        match self.phase {
            ParticipantPhase::Decided { decision, .. } => Some(decision),
            _ => None,
        }
    }

    pub fn has_voted(&self) -> bool {
        matches!(
            self.phase,
            ParticipantPhase::Voted(_) | ParticipantPhase::Decided { vote: Some(_), .. }
        )
    }
}

impl StateMachine for Participant {
    fn on_message(&mut self, msg: &Message, _at_time: u64) -> Vec<Message> {
        match (msg.message_type, self.phase) {
            (MessageType::Prepare, ParticipantPhase::Waiting) => {
                let vote = if self
                    .config
                    .rng
                    .random_bool(self.config.abort_bias.clamp(0.0, 1.0))
                {
                    Decision::Abort
                } else {
                    Decision::Commit
                };
                self.phase = ParticipantPhase::Voted(vote);

                let message_type = match vote {
                    Decision::Commit => MessageType::VoteCommit,
                    Decision::Abort => MessageType::VoteAbort,
                };
                vec![Message {
                    message_type,
                    from: ActorId::Node(self.id),
                    to: ActorId::Coordinator,
                }]
            }
            (
                MessageType::DecisionCommit | MessageType::DecisionAbort,
                ParticipantPhase::Waiting | ParticipantPhase::Voted(_),
            ) => {
                let vote = match self.phase {
                    ParticipantPhase::Voted(v) => Some(v),
                    _ => None,
                };
                let decision = if msg.message_type == MessageType::DecisionCommit {
                    Decision::Commit
                } else {
                    Decision::Abort
                };
                self.phase = ParticipantPhase::Decided { vote, decision };
                vec![]
            }
            (msg_type, phase) => {
                eprintln!(
                    "[Participant {}] Ignoring {msg_type:?} in {phase:?}",
                    self.id
                );
                vec![]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepare_msg(dest: NodeId) -> Message {
        Message {
            message_type: MessageType::Prepare,
            from: ActorId::Coordinator,
            to: ActorId::Node(dest),
        }
    }

    #[test]
    fn fixed_commit_vote() {
        let mut p = Participant::with_fixed_vote(NodeId(0), Decision::Commit);
        let msgs = p.on_message(&prepare_msg(NodeId(0)), 0);
        assert_eq!(p.phase(), ParticipantPhase::Voted(Decision::Commit));
        assert_eq!(p.vote(), Some(Decision::Commit));
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message_type, MessageType::VoteCommit);
    }

    #[test]
    fn fixed_abort_vote() {
        let mut p = Participant::with_fixed_vote(NodeId(0), Decision::Abort);
        let msgs = p.on_message(&prepare_msg(NodeId(0)), 0);
        assert_eq!(p.phase(), ParticipantPhase::Voted(Decision::Abort));
        assert_eq!(p.vote(), Some(Decision::Abort));
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message_type, MessageType::VoteAbort);
    }

    #[test]
    fn receive_decision() {
        let mut p = Participant::with_fixed_vote(NodeId(0), Decision::Commit);
        p.on_message(&prepare_msg(NodeId(0)), 0);

        let dec = Message {
            message_type: MessageType::DecisionCommit,
            from: ActorId::Coordinator,
            to: ActorId::Node(NodeId(0)),
        };
        p.on_message(&dec, 1);
        assert_eq!(p.decision(), Some(Decision::Commit));
        assert_eq!(p.vote(), Some(Decision::Commit)); // vote preserved after deciding
    }

    #[test]
    fn duplicate_prepare_ignored() {
        let mut p = Participant::with_fixed_vote(NodeId(0), Decision::Commit);
        p.on_message(&prepare_msg(NodeId(0)), 0);
        let msgs = p.on_message(&prepare_msg(NodeId(0)), 1);
        assert!(msgs.is_empty());
    }
}
