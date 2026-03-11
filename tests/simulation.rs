use proptest::prelude::*;
use proptest::property_test;

use two_phase_commit::properties;
use two_phase_commit::simulator::{ExternalEvent, Simulator};
use two_phase_commit::types::*;

// -- Event generation --

fn tick_actions_strategy(max_len: usize) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(0..=10u8, 1..=max_len)
}

fn crash_actions_strategy(max_len: usize) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(0..=30u8, 1..=max_len)
}

fn materialize_tick_events(sim: &mut Simulator, tick_deltas: &[u8]) -> u64 {
    sim.enqueue_external(ExternalEvent::StartTransaction, 0);

    let mut time: u64 = 0;
    for &delta in tick_deltas {
        time += delta as u64;
        sim.enqueue_external(ExternalEvent::TickAll, time);
    }
    time
}

fn materialize_crash_events(sim: &mut Simulator, actions: &[u8], n_participants: u8) -> u64 {
    sim.enqueue_external(ExternalEvent::StartTransaction, 0);

    let mut time: u64 = 0;
    for &action in actions {
        time += 1;
        match action {
            0..=20 => {
                sim.enqueue_external(ExternalEvent::TickAll, time);
            }
            21..=24 => {
                let node = NodeId((action - 21) % n_participants);
                sim.enqueue_external(ExternalEvent::Crash(ActorId::Node(node)), time);
            }
            25..=28 => {
                let node = NodeId((action - 25) % n_participants);
                sim.enqueue_external(ExternalEvent::Recover(ActorId::Node(node)), time);
            }
            29 => {
                sim.enqueue_external(ExternalEvent::Crash(ActorId::Coordinator), time);
            }
            30 => {
                sim.enqueue_external(ExternalEvent::Recover(ActorId::Coordinator), time);
            }
            _ => unreachable!(),
        }
    }
    time
}

// -- Property-based tests (no crashes) --

#[property_test]
fn test_safety(
    #[strategy = 1..=4u8] n_participants: u8,
    seed: u64,
    #[strategy = (0..=200u32).prop_map(|x| x as f64 / 1000.0)] abort_bias: f64,
    #[strategy = 0..=10u64] delivery_delay: u64,
    #[strategy = tick_actions_strategy(30)] tick_deltas: Vec<u8>,
) -> Result<(), TestCaseError> {
    let mut sim = Simulator::new(n_participants, seed, abort_bias, 0..delivery_delay, 5);
    let last_time = materialize_tick_events(&mut sim, &tick_deltas);

    sim.run();
    sim.drain(last_time as usize + 50);

    // step() checks invariants internally — panics on violation
    Ok(())
}

#[property_test]
fn test_termination(
    #[strategy = 1..=4u8] n_participants: u8,
    seed: u64,
    #[strategy = 0..=10u64] delivery_delay: u64,
    #[strategy = tick_actions_strategy(30)] tick_deltas: Vec<u8>,
) -> Result<(), TestCaseError> {
    let mut sim = Simulator::new(n_participants, seed, 0.0, 0..delivery_delay, 5);
    let last_time = materialize_tick_events(&mut sim, &tick_deltas);

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

// -- Property-based tests (with crashes) --

#[property_test]
fn test_safety_with_crashes(
    #[strategy = 1..=4u8] n_participants: u8,
    seed: u64,
    #[strategy = (0..=200u32).prop_map(|x| x as f64 / 1000.0)] abort_bias: f64,
    #[strategy = 0..=10u64] delivery_delay: u64,
    #[strategy = crash_actions_strategy(40)] actions: Vec<u8>,
) -> Result<(), TestCaseError> {
    let mut sim = Simulator::new(n_participants, seed, abort_bias, 0..delivery_delay, 5);
    materialize_crash_events(&mut sim, &actions, n_participants);

    sim.run();
    sim.drain(200);

    // step() checks invariants internally — panics on violation.
    // With permanent crashes, some nodes may never decide — that's fine for safety.
    Ok(())
}

#[property_test]
fn test_termination_with_crashes(
    #[strategy = 1..=4u8] n_participants: u8,
    seed: u64,
    #[strategy = 0..=10u64] delivery_delay: u64,
    #[strategy = crash_actions_strategy(40)] actions: Vec<u8>,
) -> Result<(), TestCaseError> {
    let mut sim = Simulator::new(n_participants, seed, 0.0, 0..delivery_delay, 5);
    let mut time = materialize_crash_events(&mut sim, &actions, n_participants);

    // Recovery sweep: recover all actors, then drain until quiescent.
    time += 1;
    sim.enqueue_external(ExternalEvent::Recover(ActorId::Coordinator), time);
    for i in 0..n_participants {
        time += 1;
        sim.enqueue_external(ExternalEvent::Recover(ActorId::Node(NodeId(i))), time);
    }

    sim.run();
    sim.drain(time as usize + 200);

    prop_assert!(
        properties::all_decided(sim.participants()),
        "Termination violated after crash recovery: not all participants decided.\n  Coordinator: {:?}, phase: {:?}\n  Log:\n{}",
        sim.coordinator().decision(),
        sim.coordinator().phase(),
        sim.format_log(),
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
    let mut coord = Coordinator::new(nodes.clone(), 0, abort_bias, 5);

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
