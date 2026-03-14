//! Print the event timeline for all demonstration scenarios.
//!
//! Run with: `cargo run --example timeline`

#[path = "scenarios/mod.rs"]
mod scenarios;

fn main() {
    for scenario in scenarios::all() {
        println!("=== {} ===\n", scenario.name);
        println!("{}", scenario.sim.format_log());

        let coord_decision = scenario.sim.coordinator().decision();
        println!("\nCoordinator decision: {coord_decision:?}");
        for (id, p) in scenario.sim.participants() {
            println!("  {id}: decision={:?}, vote={:?}", p.decision(), p.vote());
        }
        println!();
    }
}
