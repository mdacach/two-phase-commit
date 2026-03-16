//! The 2PC participant state machine.
//!
//! # Phase transitions
//!
//! ```text
//!       ┌─────────┐   Prepare    ┌──────────┐
//!       │ Waiting │ ───────────> │ Voted(v) │
//!       └────┬────┘              └─────┬────┘
//!            │ Decision(X)             │ Decision(X)
//!            │ (before voting)         │
//!            v                         v
//!  ┌─────────────────┐   ┌─────────────────────—┐
//!  │ Decided         │   │ Decided              │
//!  │   vote: None    │   │   vote: Some(v)      │
//!  │   decision: X   │   │   decision: X        │
//!  └─────────────────┘   └──────────────────────┘
//! ```
//!
//! A participant can receive the coordinator's decision *before* it has voted,
//! or even before it received Prepare — for example if another participant's
//! abort caused an early decision.
//!
//! # Crash recovery
//!
//! On [`recover`](StateMachine::recover), the participant restores its phase
//! from [`DurableState`]:
//! - Durable decision → [`Decided`](ParticipantPhase::Decided).
//! - Durable vote only → [`Voted`](ParticipantPhase::Voted).
//! - Neither → [`Waiting`](ParticipantPhase::Waiting).
//!
//! # Idempotent re-send
//!
//! - Duplicate Prepare while in `Voted`: re-sends the original vote.
//! - Duplicate Decision while in `Decided`: re-sends Ack.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use tracing::warn;

use crate::state_machine::StateMachine;
use crate::types::*;

/// The participant's phase in the protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipantPhase {
    /// Participant is waiting for a `Prepare` message from the coordinator.
    Waiting,
    /// Participant has voted and is waiting for coordinator's final decision.
    Voted(Vote),
    /// Participant has received the coordinator's decision.
    ///
    /// `vote` is `None` if the decision arrived before Prepare (so the
    /// participant never voted).
    Decided {
        vote: Option<Vote>,
        decision: Decision,
    },
}

/// Simulation configuration.
struct Config {
    rng: ChaCha8Rng,
    abort_bias: f64,
}

/// Durable state that survives crashes. Written before sending messages,
/// read on recovery.
struct DurableState {
    /// Persisted so the participant can re-send its vote after a crash
    /// (the coordinator may still be collecting votes).
    vote: Option<Vote>,
    /// Persisted so the participant recovers directly into `Decided`
    /// rather than blocking in `Voted` waiting for a retransmission.
    decision: Option<Decision>,
}

/// A participant in the two-phase commit protocol.
///
/// Participants vote on whether to commit or abort, then wait for the
/// coordinator's decision. See the [module docs](self) for the full
/// phase-transition diagram and crash-recovery semantics.
pub struct Participant {
    id: NodeId,
    phase: ParticipantPhase,
    durable_state: DurableState,
    config: Config,
}

impl Participant {
    /// Create a participant with the given `id` and abort probability.
    ///
    /// - `rng_seed` — deterministic seed for the vote coin flip.
    /// - `abort_bias` — probability that this participant votes Abort on Prepare.
    pub fn new(id: NodeId, rng_seed: u64, abort_bias: f64) -> Self {
        Self {
            id,
            phase: ParticipantPhase::Waiting,
            durable_state: DurableState {
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
    pub fn with_fixed_vote(id: NodeId, vote: Vote) -> Self {
        let abort_bias = match vote {
            Vote::Commit => 0.0,
            Vote::Abort => 1.0,
        };
        Self::new(id, 0, abort_bias)
    }

    /// Current protocol phase.
    pub fn phase(&self) -> ParticipantPhase {
        self.phase
    }

    /// The vote this participant cast, if any.
    pub fn vote(&self) -> Option<Vote> {
        match self.phase {
            ParticipantPhase::Voted(v) => Some(v),
            ParticipantPhase::Decided { vote, .. } => vote,
            ParticipantPhase::Waiting => None,
        }
    }

    /// The decision received from the coordinator, if any.
    pub fn decision(&self) -> Option<Decision> {
        match self.phase {
            ParticipantPhase::Decided { decision, .. } => Some(decision),
            _ => None,
        }
    }

    /// Whether this participant has cast a vote (either commit or abort).
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

    fn make_vote_msg(&self, vote: Vote) -> Message {
        let message_type = match vote {
            Vote::Commit => MessageType::VoteCommit,
            Vote::Abort => MessageType::VoteAbort,
        };
        Message {
            message_type,
            from: ActorId::Node(self.id),
            to: ActorId::Coordinator,
        }
    }
}

impl StateMachine for Participant {
    fn on_message(&mut self, msg: &Message, at_time: u64) -> Vec<Message> {
        let mut outgoing = self.tick(at_time);

        match (msg.message_type, self.phase) {
            (MessageType::Prepare, ParticipantPhase::Waiting) => {
                let vote = if self
                    .config
                    .rng
                    .random_bool(self.config.abort_bias.clamp(0.0, 1.0))
                {
                    Vote::Abort
                } else {
                    Vote::Commit
                };
                self.durable_state.vote = Some(vote);
                self.phase = ParticipantPhase::Voted(vote);
                outgoing.push(self.make_vote_msg(vote));
            }
            // Duplicate Prepare while already voted: re-send vote.
            //
            // This occurs after a coordinator crash and recovery: the
            // coordinator re-enters Voting with cleared votes and retransmits
            // Prepare. The participant must re-send its original vote so the
            // coordinator can re-collect votes and decide.
            (MessageType::Prepare, ParticipantPhase::Voted(vote)) => {
                outgoing.push(self.make_vote_msg(vote));
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
                self.durable_state.decision = Some(decision);
                self.phase = ParticipantPhase::Decided { vote, decision };
                outgoing.push(self.make_ack());
            }
            // Duplicate Decision while already decided: re-send Ack.
            //
            // This occurs after a coordinator crash and recovery: the
            // coordinator re-enters AwaitingAcks and retransmits Decision.
            // The participant must re-send its Ack so the coordinator can
            // complete the protocol.
            (
                MessageType::DecisionCommit | MessageType::DecisionAbort,
                ParticipantPhase::Decided { .. },
            ) => {
                outgoing.push(self.make_ack());
            }
            (msg_type, phase) => {
                warn!(participant = %self.id, ?msg_type, ?phase, "Ignoring unexpected message");
            }
        }

        outgoing
    }

    /// No-op: the participant is purely reactive. Unlike the coordinator, it
    /// has no retransmission timer and no spontaneous actions — it only
    /// produces messages in response to Prepare or Decision from the
    /// coordinator.
    fn tick(&mut self, _at_time: u64) -> Vec<Message> {
        vec![]
    }

    /// Restore phase from durable storage after a crash. Without this,
    /// a participant that voted would forget its vote and potentially
    /// vote differently on a retransmitted Prepare.
    fn recover(&mut self, _at_time: u64) {
        if let Some(decision) = self.durable_state.decision {
            self.phase = ParticipantPhase::Decided {
                vote: self.durable_state.vote,
                decision,
            };
        } else if let Some(vote) = self.durable_state.vote {
            self.phase = ParticipantPhase::Voted(vote);
        } else {
            self.phase = ParticipantPhase::Waiting;
        }
    }

    /// Quiescent in `Waiting` (not yet part of a transaction) or `Decided`
    /// (final state — will only re-send Ack if prompted).
    fn is_quiescent(&self) -> bool {
        matches!(
            self.phase,
            ParticipantPhase::Decided { .. } | ParticipantPhase::Waiting
        )
    }
}

#[cfg(test)]
mod tests;
