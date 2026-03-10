use proptest::prelude::*;
use proptest::property_test;

use two_phase_commit::properties;
use two_phase_commit::simulator::{ExternalEvent, Simulator};
use two_phase_commit::types::*;

// -- Event generation --

fn actions_strategy(max_len: usize) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(0..=10u8, 1..=max_len)
}

fn materialize_events(sim: &mut Simulator, tick_deltas: &[u8]) -> u64 {
    // Always start with a single StartTransaction at time 0.
    sim.enqueue_external(ExternalEvent::StartTransaction, 0);

    let mut time: u64 = 0;
    for &delta in tick_deltas {
        time += delta as u64;
        sim.enqueue_external(ExternalEvent::TickAll, time);
    }
    time
}

// -- Property-based tests --

#[property_test]
fn test_safety(
    #[strategy = 1..=4u8] n_participants: u8,
    seed: u64,
    #[strategy = (0..=200u32).prop_map(|x| x as f64 / 1000.0)] abort_bias: f64,
    #[strategy = 0..=10u64] delivery_delay: u64,
    #[strategy = actions_strategy(30)] tick_deltas: Vec<u8>,
) -> Result<(), TestCaseError> {
    let mut sim = Simulator::new(n_participants, seed, abort_bias, 0..delivery_delay);
    let last_time = materialize_events(&mut sim, &tick_deltas);

    sim.run();
    // Drain remaining events to let the protocol settle.
    sim.drain(last_time as usize + 50);

    // step() checks invariants internally — panics on violation
    Ok(())
}

#[property_test]
fn test_termination(
    #[strategy = 1..=4u8] n_participants: u8,
    seed: u64,
    #[strategy = 0..=10u64] delivery_delay: u64,
    #[strategy = actions_strategy(30)] tick_deltas: Vec<u8>,
) -> Result<(), TestCaseError> {
    // No abort bias — ensures coordinator always commits or aborts based on votes,
    // never spontaneously aborts (which could fire before StartTransaction is delivered).
    let mut sim = Simulator::new(n_participants, seed, 0.0, 0..delivery_delay);
    let last_time = materialize_events(&mut sim, &tick_deltas);

    sim.run();
    sim.drain(last_time as usize + 100);

    prop_assert!(
        properties::all_decided(sim.participants()),
        "Termination violated: not all participants decided. Coordinator: {:?}, phase: {:?}",
        sim.coordinator().decision(),
        sim.coordinator().phase(),
    );

    Ok(())
}

// -- Deterministic edge-case tests --

/// Drive the protocol to completion with fixed votes using a simple message queue.
fn manual_protocol(votes: &[Decision], abort_bias: f64) {
    use two_phase_commit::coordinator::{Coordinator, CoordinatorPhase};
    use two_phase_commit::participant::Participant;
    use two_phase_commit::state_machine::StateMachine;

    let nodes: Vec<NodeId> = (0..votes.len() as u8).map(NodeId).collect();
    let mut coord = Coordinator::new(nodes.clone(), 0, abort_bias);

    let mut participants: Vec<Participant> = votes
        .iter()
        .enumerate()
        .map(|(i, &v)| Participant::with_fixed_vote(NodeId(i as u8), v))
        .collect();

    // Drive the protocol with a simple message queue.
    let mut queue: Vec<Message> = vec![Message {
        message_type: MessageType::StartTransaction,
        from: ActorId::Coordinator,
        to: ActorId::Coordinator,
    }];

    let mut time: u64 = 0;
    while !queue.is_empty() || !matches!(coord.phase(), CoordinatorPhase::Done(_)) {
        time += 1;

        let batch = std::mem::take(&mut queue);
        for msg in batch {
            let outgoing = match msg.to {
                ActorId::Coordinator => coord.on_message(&msg, time),
                ActorId::Node(id) => participants[id.0 as usize].on_message(&msg, time),
            };
            queue.extend(outgoing);
        }

        // If no messages pending and protocol not done, tick to advance.
        if queue.is_empty() && !matches!(coord.phase(), CoordinatorPhase::Done(_)) {
            queue.extend(coord.tick(time));
            for p in &mut participants {
                queue.extend(p.tick(time));
            }
        }

        assert!(time < 100, "Protocol did not terminate within 100 steps");
    }

    // Check properties.
    let part_map: std::collections::BTreeMap<NodeId, Participant> = participants
        .into_iter()
        .enumerate()
        .map(|(i, p)| (NodeId(i as u8), p))
        .collect();

    properties::check_all_invariants(&coord, &part_map).expect("Invariant violated");
    assert!(
        properties::all_decided(&part_map),
        "Not all participants decided"
    );

    // Verify expected outcomes.
    let expected_decision = if abort_bias >= 1.0 {
        Decision::Abort
    } else if votes.iter().any(|v| *v == Decision::Abort) {
        Decision::Abort
    } else {
        Decision::Commit
    };

    assert_eq!(coord.decision(), Some(expected_decision));
    for p in part_map.values() {
        assert_eq!(p.decision(), Some(expected_decision));
    }
}

#[test]
fn all_commit() {
    manual_protocol(&[Decision::Commit, Decision::Commit], 0.0);
}

#[test]
fn one_abort() {
    manual_protocol(&[Decision::Commit, Decision::Abort], 0.0);
}

#[test]
fn all_abort() {
    manual_protocol(&[Decision::Abort, Decision::Abort], 0.0);
}

#[test]
fn coordinator_abort_despite_all_commits() {
    manual_protocol(&[Decision::Commit, Decision::Commit], 1.0);
}

#[test]
fn single_participant_commit() {
    manual_protocol(&[Decision::Commit], 0.0);
}

#[test]
fn single_participant_abort() {
    manual_protocol(&[Decision::Abort], 0.0);
}
