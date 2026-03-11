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
//! # Idempotent message handling
//!
//! - Duplicate Prepare while in `Voted`: re-send the original vote.
//! - Duplicate Decision while in `Decided`: re-send Ack.
//!
//! # Crash recovery (WAL)
//!
//! The participant holds a `Wal` struct with `vote: Option<Decision>` and
//! `decision: Option<Decision>`. On `recover()`:
//! - If `wal.decision` is `Some(d)`: reset to `Decided { vote: wal.vote, decision: d }`.
//! - Else if `wal.vote` is `Some(v)`: reset to `Voted(v)`.
//! - Else: reset to `Waiting`.
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

struct Wal {
    vote: Option<Decision>,
    decision: Option<Decision>,
}

pub struct Participant {
    id: NodeId,
    phase: ParticipantPhase,
    wal: Wal,
    config: Config,
}

impl Participant {
    pub fn new(id: NodeId, rng_seed: u64, abort_bias: f64) -> Self {
        Self {
            id,
            phase: ParticipantPhase::Waiting,
            wal: Wal {
                vote: None,
                decision: None,
            },
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

    fn make_ack(&self) -> Message {
        Message {
            message_type: MessageType::Ack,
            from: ActorId::Node(self.id),
            to: ActorId::Coordinator,
        }
    }

    fn make_vote_msg(&self, vote: Decision) -> Message {
        let message_type = match vote {
            Decision::Commit => MessageType::VoteCommit,
            Decision::Abort => MessageType::VoteAbort,
        };
        Message {
            message_type,
            from: ActorId::Node(self.id),
            to: ActorId::Coordinator,
        }
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
                self.wal.vote = Some(vote);
                self.phase = ParticipantPhase::Voted(vote);
                vec![self.make_vote_msg(vote)]
            }
            // Duplicate Prepare while already voted: re-send vote.
            (MessageType::Prepare, ParticipantPhase::Voted(vote)) => {
                vec![self.make_vote_msg(vote)]
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
                self.wal.decision = Some(decision);
                self.phase = ParticipantPhase::Decided { vote, decision };
                vec![self.make_ack()]
            }
            // Duplicate Decision while already decided: re-send Ack.
            (
                MessageType::DecisionCommit | MessageType::DecisionAbort,
                ParticipantPhase::Decided { .. },
            ) => {
                vec![self.make_ack()]
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

    fn is_quiescent(&self) -> bool {
        matches!(
            self.phase,
            ParticipantPhase::Decided { .. } | ParticipantPhase::Waiting
        )
    }

    fn recover(&mut self, _at_time: u64) {
        if let Some(decision) = self.wal.decision {
            self.phase = ParticipantPhase::Decided {
                vote: self.wal.vote,
                decision,
            };
        } else if let Some(vote) = self.wal.vote {
            self.phase = ParticipantPhase::Voted(vote);
        } else {
            self.phase = ParticipantPhase::Waiting;
        }
    }
}

#[cfg(test)]
mod tests;
