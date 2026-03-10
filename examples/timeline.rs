//! Print the event timeline for a few 2PC scenarios.

use two_phase_commit::simulator::{ExternalEvent, Simulator};

fn run_scenario(label: &str, n_participants: u8, seed: u64, abort_bias: f64, delivery_delay: u64) {
    println!("=== {label} ===\n");

    let mut sim = Simulator::new(n_participants, seed, abort_bias, 0..delivery_delay);
    sim.enqueue_external(ExternalEvent::StartTransaction, 0);
    sim.run();
    sim.drain(50);

    println!("{}", sim.format_log());

    let coord_decision = sim.coordinator().decision();
    println!("\nCoordinator decision: {coord_decision:?}");
    for (id, p) in sim.participants() {
        println!("  {id}: decision={:?}, vote={:?}", p.decision(), p.vote());
    }
    println!();
}

fn main() {
    // abort_bias=0.0 for participants means all vote commit (Simulator hardcodes participant abort_bias=0.2,
    // so we rely on seed choice for deterministic votes — seed 0 gives all-commit for 2 participants).
    run_scenario("2 participants, no delay", 2, 0, 0.0, 0);
    run_scenario("3 participants, no delay", 3, 7, 0.0, 0);
    run_scenario("3 participants, delay 0..5", 3, 7, 0.0, 5);
}
