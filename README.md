# Two-Phase Commit: Formal Specification and Simulation

A study of the Two-Phase Commit (2PC) protocol, using three approaches:

1. **TLA+** (`tla/TwoPhaseCommit.tla`) — safety and liveness model-checked
   with TLC.
2. **Alloy 6** (`alloy/TwoPhaseCommit.als`) — bounded verification with
   temporal operators.
3. **Rust** (`src/`) — deterministic discrete-event simulation with crash
   recovery, and property-based testing.

Each verifies the same core properties:

- **Agreement (AC1):** All decided participants reach the same decision.
- **Validity (AC2):** Commit requires unanimous commit votes.
- **Termination:** Under fair scheduling (and eventual recovery), all
  participants eventually decide.

## Goals

This project is a learning exercise in formal specification and
deterministic simulation testing. The primary goals were to practice
writing and model-checking TLA+ specifications, bounded verification
with Alloy 6, and deterministic discrete-event simulation in Rust.
Two-phase commit is a small enough protocol to spec and simulate
end-to-end while still exercising interesting failure modes (coordinator
crash, message loss, retransmission).

A secondary goal was testing AI-assisted development with informal
specs, including using Claude Code skills for learning opportunities
and authoring assessments/quizzes during the development process.

### Observations on AI-assisted development

Current models struggled with less common tools like Alloy and subtle
subjects like temporal logic, making a considerable number of mistakes.
Overall, code produced was of slightly less than average quality, and
review rounds took significantly longer than expected.

That said, the results were positive on balance. AI assistance enabled
tasks I would not have attempted otherwise, like the interactive HTML
visualization examples. I estimate the code after review rounds is only
slightly lower quality than what I would have produced by hand, but
probably significantly worse than what an expert would write.

Understanding is harder to evaluate. Even with assessment quizzes and
learning-opportunity exercises, I likely took longer to understand the
code and understand it less deeply than if I had written it all by hand.

## Examples

### Happy path

All participants vote Commit, coordinator commits:

![Happy path — unanimous commit](examples/happy-path.gif)

### Coordinator crash and recovery

The coordinator crashes after deciding but before collecting Acks.
After recovery it restores its decision from durable state, retransmits,
and completes the protocol:

![Coordinator crash and recovery](examples/coordinator-crash.gif)

This demonstrates the well-known blocking vulnerability of 2PC:
participants cannot make progress until the coordinator recovers.

Run the examples yourself:

```sh
cargo run --example timeline     # text timelines for all scenarios
cargo run --example visualize    # interactive HTML trace (opens in browser)
```

## Design

The simulation design is inspired by the "theater of state machines"
model described in [Sled's simulation guide][sled-sim]: protocol actors
are state machines that communicate exclusively through message passing,
driven by a central simulator that acts as a message bus and scheduler.

[sled-sim]: https://sled.rs/simulation.html

Actors implement a `StateMachine` trait:

- **`on_message`** — react to an inbound message, return outgoing messages.
- **`tick`** — time-driven actions (retransmission, spontaneous abort).
- **`recover`** — restore from durable state after a crash (needed for
  the simulator to crash and restart actors mid-protocol).
- **`is_quiescent`** — report whether the actor is in a terminal state.

The `Simulator` drives actors through a min-heap event queue with
configurable delivery delay and crash injection. Safety invariants
(agreement + validity) are checked after every event.

### Crash recovery

To simulate node crashes, coordinator and participants persist critical
state to a `DurableState` struct before sending messages. On recovery,
actors restore their phase from durable state (but nothing else) and
resume the protocol.

## Building and testing

```sh
cargo test                       # unit + property-based tests (proptest)
cargo doc --open                 # browse API documentation
```

## Project structure

```
tla/
  TwoPhaseCommit.tla             TLA+ specification (no-failure model)
  TwoPhaseCommit.cfg             TLC model-checking configuration
alloy/
  TwoPhaseCommit.als             Alloy 6 specification (no-failure model)
src/
  lib.rs                         Crate root and architecture overview
  types.rs                       Core types (NodeId, Vote, Decision, Message)
  state_machine.rs               StateMachine trait (on_message, tick, recover)
  coordinator/                   Coordinator state machine
  participant/                   Participant state machine
  simulator/
    mod.rs                       Discrete-event simulator with crash injection
    event.rs                     Event queue with deterministic ordering
    properties.rs                Safety invariants checked after every step
tests/
  simulation.rs                  Property-based tests with proptest
examples/
  timeline.rs                    Text-based protocol traces
  visualize.rs                   Interactive HTML visualization
  scenarios/                     Pre-built demonstration scenarios
.specs/                          Specification and planning documents
```
