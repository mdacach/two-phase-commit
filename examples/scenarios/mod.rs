//! Pre-built protocol scenarios for examples and demonstrations.
//!
//! Each scenario constructs a [`Simulator`], enqueues external events, runs the
//! protocol to quiescence, and returns a named scenario.  Both the `timeline`
//! and `visualize` examples consume these via `scenarios::all()`.

use two_phase_commit::simulator::{ExternalEvent, Simulator};
use two_phase_commit::types::*;

/// A named, already-executed simulation scenario.
pub struct Scenario {
    pub name: &'static str,
    pub sim: Simulator,
}

/// All demonstration scenarios, in display order.
pub fn all() -> Vec<Scenario> {
    vec![
        happy_path(),
        three_participants_with_delay(),
        participant_votes_abort(),
        coordinator_crash_before_decision(),
        coordinator_crash_after_decision(),
        participant_crash_after_voting(),
    ]
}

/// Happy path: all participants vote commit, coordinator commits.
/// Two participants, zero delivery delay, seed chosen to produce all-commit
/// votes (participant abort_bias is 0.2, so the outcome is seed-dependent).
fn happy_path() -> Scenario {
    let mut sim = Simulator::new(2, 1, 0.0, 0.2, 0..0, 5);
    sim.enqueue_external(ExternalEvent::StartTransaction, 0);
    sim.run();
    sim.drain(50);
    Scenario {
        name: "Happy path — unanimous commit",
        sim,
    }
}

/// Three participants with random delivery delay (0..5).
/// Seed 7 causes Node(2) to vote abort, so the coordinator aborts despite
/// the other two participants voting commit.
fn three_participants_with_delay() -> Scenario {
    let mut sim = Simulator::new(3, 7, 0.0, 0.2, 0..5, 5);
    sim.enqueue_external(ExternalEvent::StartTransaction, 0);
    sim.run();
    sim.drain(50);
    Scenario {
        name: "Three participants with network delay",
        sim,
    }
}

/// A participant votes abort, forcing the coordinator to abort the
/// transaction even though the other participant voted commit.
/// Two participants, zero delay. Seed 15 causes Node(1) to vote abort.
fn participant_votes_abort() -> Scenario {
    let mut sim = Simulator::new(2, 15, 0.0, 0.2, 0..0, 5);
    sim.enqueue_external(ExternalEvent::StartTransaction, 0);
    sim.run();
    sim.drain(50);
    Scenario {
        name: "One abort vote forces rollback",
        sim,
    }
}

/// Coordinator crashes after sending Prepare but before receiving any votes.
/// Votes in flight are dropped. After recovery, the coordinator retransmits
/// Prepare, re-collects votes, and completes the protocol.
fn coordinator_crash_before_decision() -> Scenario {
    let mut sim = Simulator::new(2, 1, 0.0, 0.2, 0..0, 5);
    sim.enqueue_external(ExternalEvent::StartTransaction, 0);
    // Crash at t=2: after Prepare delivered (t=1) but before votes arrive (t=2).
    // External events get lower sequence numbers than internal events at the same
    // timestamp, so the Crash fires before the vote deliveries.
    sim.enqueue_external(ExternalEvent::Crash(ActorId::Coordinator), 2);
    sim.enqueue_external(ExternalEvent::Recover(ActorId::Coordinator), 8);
    sim.run();
    sim.drain(50);
    Scenario {
        name: "Coordinator crashes before deciding",
        sim,
    }
}

/// Coordinator decides and sends Decision, then crashes before receiving Acks.
/// Acks in flight are dropped. After recovery the coordinator re-enters
/// Decided (from WAL), retransmits Decision, re-collects Acks, and completes.
fn coordinator_crash_after_decision() -> Scenario {
    let mut sim = Simulator::new(2, 1, 0.0, 0.2, 0..0, 5);
    sim.enqueue_external(ExternalEvent::StartTransaction, 0);
    // Crash at t=4: Decision messages delivered at t=3, Acks sent at t=3
    // and scheduled for t=4. Crash fires before Ack deliveries.
    sim.enqueue_external(ExternalEvent::Crash(ActorId::Coordinator), 4);
    sim.enqueue_external(ExternalEvent::Recover(ActorId::Coordinator), 10);
    sim.run();
    sim.drain(50);
    Scenario {
        name: "Coordinator crashes after deciding",
        sim,
    }
}

/// A participant crashes after voting but before receiving the Decision.
/// The coordinator decides normally, but the Decision to the dead participant
/// is dropped. After recovery the participant is back in Voted (from WAL);
/// the coordinator retransmits Decision, the participant Acks, and the
/// protocol completes.
fn participant_crash_after_voting() -> Scenario {
    let node0 = ActorId::Node(NodeId(0));
    let mut sim = Simulator::new(2, 1, 0.0, 0.2, 0..0, 5);
    sim.enqueue_external(ExternalEvent::StartTransaction, 0);
    // Node(0) crashes at t=3: votes were delivered to coordinator at t=2,
    // Decision sent at t=2, scheduled for delivery at t=3. Crash fires
    // before the Decision delivery (external events have lower seq numbers).
    sim.enqueue_external(ExternalEvent::Crash(node0), 3);
    sim.enqueue_external(ExternalEvent::Recover(node0), 10);
    sim.run();
    sim.drain(50);
    Scenario {
        name: "Participant crashes after voting",
        sim,
    }
}
