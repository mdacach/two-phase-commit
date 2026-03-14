//! Safety invariants checked after every simulator step.
//!
//! Properties are checked against wire-level [`Observations`] — votes,
//! coordinator decisions, and participant decisions recorded by observing
//! messages at send and delivery time. This makes property checking
//! independent of actor-internal state and robust to scenarios like crash
//! recovery that reset volatile state.
//!
//! # TLA+ correspondence
//!
//! | Rust                 | TLA+              | Property (Babaoglu-Toueg) |
//! |----------------------|-------------------|--------------------------|
//! | `check_agreement`    | `Agreement`       | AC1 — all decided participants agree |
//! | `check_validity` (commit arm) | `Consistency` | AC2 — commit requires unanimous commit votes |
//! | `check_validity` (abort arm)  | *(none)*  | If coordinator aborted, no participant committed |
//!
//! The abort arm is weaker than textbook abort-validity (NBAC4: "abort only if
//! some participant voted no *or crashed*") because the Rust model's `abort_bias`
//! lets the coordinator legitimately abort despite unanimous commit votes.

use std::collections::BTreeMap;

use crate::types::*;

/// Wire-level observations collected during simulation.
///
/// All fields are populated by observing messages at send or delivery time,
/// without inspecting internal actor state. This makes property checking
/// independent of coordinator/participant implementation details and robust
/// to crash recovery (which resets volatile state).
///
/// # Recording points
///
/// - **Votes**: recorded when a `VoteCommit`/`VoteAbort` message is *sent*
///   by a participant. Send-time recording means the observation survives
///   coordinator crashes that clear the coordinator's volatile vote map.
/// - **Coordinator decision**: recorded when a `DecisionCommit`/`DecisionAbort`
///   message is *sent* by the coordinator. Consistency is verified across
///   all sends (including retransmissions after crash recovery).
/// - **Participant decisions**: recorded when a `DecisionCommit`/`DecisionAbort`
///   message is *delivered* to a participant (and actually processed, not
///   dropped due to a crash).
#[derive(Debug, Clone, Default)]
pub struct Observations {
    /// Votes observed when sent by participants.
    votes: BTreeMap<NodeId, Vote>,
    /// The decision the coordinator has sent, verified consistent across all
    /// sends including after crash recovery.
    coordinator_decision: Option<Decision>,
    /// Decisions delivered to (and processed by) participants.
    participant_decisions: BTreeMap<NodeId, Decision>,
}

impl Observations {
    /// Create an empty set of observations.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record any relevant observations from a sent message.
    ///
    /// Call this at send time for every outgoing message. Captures:
    /// - Participant votes (`VoteCommit`/`VoteAbort` from a node)
    /// - Coordinator decisions (`DecisionCommit`/`DecisionAbort` from coordinator)
    pub fn record_sent(&mut self, msg: &Message) {
        match (&msg.message_type, msg.from) {
            (MessageType::VoteCommit, ActorId::Node(id)) => {
                self.record_vote(id, Vote::Commit);
            }
            (MessageType::VoteAbort, ActorId::Node(id)) => {
                self.record_vote(id, Vote::Abort);
            }
            (MessageType::DecisionCommit, ActorId::Coordinator) => {
                self.record_coordinator_decision(Decision::Commit);
            }
            (MessageType::DecisionAbort, ActorId::Coordinator) => {
                self.record_coordinator_decision(Decision::Abort);
            }
            _ => {}
        }
    }

    /// Record any relevant observations from a delivered message.
    ///
    /// Call this at delivery time for every message that is actually
    /// processed (not dropped due to a crash). Captures participant
    /// decisions (`DecisionCommit`/`DecisionAbort` delivered to a node).
    pub fn record_delivered(&mut self, msg: &Message) {
        match (&msg.message_type, msg.to) {
            (MessageType::DecisionCommit, ActorId::Node(id)) => {
                self.record_participant_decision(id, Decision::Commit);
            }
            (MessageType::DecisionAbort, ActorId::Node(id)) => {
                self.record_participant_decision(id, Decision::Abort);
            }
            _ => {}
        }
    }

    /// Record a vote sent by a participant.
    ///
    /// Panics if the same participant sends a different vote (which would
    /// indicate a protocol bug — votes must be deterministic and durable).
    fn record_vote(&mut self, node: NodeId, vote: Vote) {
        if let Some(&existing) = self.votes.get(&node) {
            assert_eq!(
                existing, vote,
                "Participant {node} sent conflicting votes: {existing:?} then {vote:?}"
            );
            return;
        }
        self.votes.insert(node, vote);
    }

    /// Record a decision sent by the coordinator.
    ///
    /// Panics if the coordinator sends a different decision than previously
    /// observed (which would indicate a protocol bug — the durable decision
    /// must be consistent across crash recovery).
    fn record_coordinator_decision(&mut self, decision: Decision) {
        if let Some(existing) = self.coordinator_decision {
            assert_eq!(
                existing, decision,
                "Coordinator sent conflicting decisions: {existing:?} then {decision:?}"
            );
            return;
        }
        self.coordinator_decision = Some(decision);
    }

    /// Record a decision delivered to a participant.
    ///
    /// Panics if the participant receives a different decision than previously
    /// delivered (which would indicate a routing or coordinator bug).
    fn record_participant_decision(&mut self, node: NodeId, decision: Decision) {
        if let Some(&existing) = self.participant_decisions.get(&node) {
            assert_eq!(
                existing, decision,
                "Participant {node} received conflicting decisions: {existing:?} then {decision:?}"
            );
            return;
        }
        self.participant_decisions.insert(node, decision);
    }

    /// Votes observed so far.
    pub fn votes(&self) -> &BTreeMap<NodeId, Vote> {
        &self.votes
    }

    /// The coordinator's decision, if observed.
    pub fn coordinator_decision(&self) -> Option<Decision> {
        self.coordinator_decision
    }

    /// Decisions delivered to participants.
    pub fn participant_decisions(&self) -> &BTreeMap<NodeId, Decision> {
        &self.participant_decisions
    }
}

/// AC1: all participants that have received a decision must agree on the
/// same value.
pub fn check_agreement(obs: &Observations) -> Result<(), String> {
    if obs.participant_decisions.len() < 2 {
        return Ok(());
    }

    let committed: Vec<NodeId> = obs
        .participant_decisions
        .iter()
        .filter(|(_, d)| **d == Decision::Commit)
        .map(|(id, _)| *id)
        .collect();
    let aborted: Vec<NodeId> = obs
        .participant_decisions
        .iter()
        .filter(|(_, d)| **d == Decision::Abort)
        .map(|(id, _)| *id)
        .collect();

    if !committed.is_empty() && !aborted.is_empty() {
        return Err(format!(
            "Agreement violated: committed={committed:?}, aborted={aborted:?}"
        ));
    }
    Ok(())
}

/// AC2 (commit direction) + abort-safety (abort direction).
///
/// - **Commit**: coordinator sent a commit decision → all observed votes
///   are Commit. Votes are tracked at send time (not delivery), so the
///   check survives coordinator crashes that clear volatile votes.
///   Corresponds to TLA+ `Consistency`.
/// - **Abort**: coordinator sent an abort decision → no participant has
///   received a commit decision.
pub fn check_validity(obs: &Observations) -> Result<(), String> {
    match obs.coordinator_decision {
        Some(Decision::Commit) => {
            for (id, vote) in &obs.votes {
                if *vote != Vote::Commit {
                    return Err(format!(
                        "Validity violated: coordinator committed but {id} voted {vote:?}"
                    ));
                }
            }
        }
        Some(Decision::Abort) => {
            for (id, d) in &obs.participant_decisions {
                if *d == Decision::Commit {
                    return Err(format!(
                        "Validity violated: coordinator aborted but {id} committed"
                    ));
                }
            }
        }
        None => {}
    }
    Ok(())
}

/// Check all safety invariants (agreement + validity). Returns the first
/// violation found, if any.
pub fn check_all_invariants(obs: &Observations) -> Result<(), String> {
    check_agreement(obs)?;
    check_validity(obs)?;
    Ok(())
}

/// Returns `true` if every participant in `participants` has received a
/// decision.
pub fn all_decided(obs: &Observations, participants: &[NodeId]) -> bool {
    participants
        .iter()
        .all(|id| obs.participant_decisions.contains_key(id))
}
