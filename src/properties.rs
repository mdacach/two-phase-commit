//! Safety invariants checked after every simulator step.
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

use crate::coordinator::Coordinator;
use crate::participant::Participant;
use crate::types::*;

/// AC1: all participants that have decided must agree on the same value.
pub fn check_agreement(participants: &BTreeMap<NodeId, Participant>) -> Result<(), String> {
    let decisions: Vec<(NodeId, Decision)> = participants
        .iter()
        .filter_map(|(&id, p)| p.decision().map(|d| (id, d)))
        .collect();

    if decisions.len() < 2 {
        return Ok(());
    }

    let committed: Vec<NodeId> = decisions
        .iter()
        .filter(|(_, d)| *d == Decision::Commit)
        .map(|(id, _)| *id)
        .collect();
    let aborted: Vec<NodeId> = decisions
        .iter()
        .filter(|(_, d)| *d == Decision::Abort)
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
/// - **Commit**: coordinator committed → all votes are Commit and all votes
///   are in. Corresponds to TLA+ `Consistency`.
/// - **Abort**: coordinator aborted → no participant has committed.
pub fn check_validity(
    coordinator: &Coordinator,
    participants: &BTreeMap<NodeId, Participant>,
) -> Result<(), String> {
    match coordinator.decision() {
        Some(Decision::Commit) => {
            if coordinator.votes().len() != coordinator.nodes().len() {
                return Err(format!(
                    "Validity violated: coordinator committed with {}/{} votes",
                    coordinator.votes().len(),
                    coordinator.nodes().len()
                ));
            }
            for (id, vote) in coordinator.votes() {
                if *vote != Decision::Commit {
                    return Err(format!(
                        "Validity violated: coordinator committed but {id} voted {vote:?}"
                    ));
                }
            }
        }
        Some(Decision::Abort) => {
            for (id, p) in participants {
                if p.decision() == Some(Decision::Commit) {
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

pub fn check_all_invariants(
    coordinator: &Coordinator,
    participants: &BTreeMap<NodeId, Participant>,
) -> Result<(), String> {
    check_agreement(participants)?;
    check_validity(coordinator, participants)?;
    Ok(())
}

pub fn all_decided(participants: &BTreeMap<NodeId, Participant>) -> bool {
    participants.values().all(|p| p.decision().is_some())
}
