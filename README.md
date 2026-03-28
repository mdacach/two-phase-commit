# Two-Phase Commit: Formal Specification and Simulation

An exploration of the Two-Phase Commit (2PC) protocol:

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

This project is a learning exercise in formal specification and deterministic
simulation testing. It follows a [reliable channel simulation][rel-chan] that
used the same three-pronged approach (TLA+, Alloy, Rust DST) on a simpler
stop-and-wait protocol.

A secondary goal was to practice AI-assisted development, including
using Claude Code skills for [learning opportunities][learn-opp] and
authoring assessments/quizzes during the development process.

[rel-chan]: https://github.com/mdacach/dst-reliable-channel
[learn-opp]: https://github.com/DrCatHicks/learning-opportunities

### Observations on AI-assisted development

Even the current strong models struggle with less common tools like Alloy and
subtle subjects like temporal logic. I caught a considerable number of mistakes
(and probably missed a few more D:). LLM-churned code was maybe slightly
less-than-average quality. My review rounds took significantly longer than
expected (and were more draining than expected, too), but resulting code then
was decent. A Rust expert would write significantly better code — but I am not
one of those myself. That being said, the code produced was rarely straight-up
incorrect.

Overall I would say the results were positive, primarily because AI assistance
enabled tasks I would not have attempted otherwise, like the interactive HTML
visualization examples.

My learning is harder to evaluate. Even with assessment quizzes and
learning-opportunity exercises, I feel I might have taken longer to understand
the code and understand it less deeply than if I had written it by hand.

## Examples

### Happy path

All participants vote Commit, coordinator commits:

![Happy path — unanimous commit](examples/happy-path.gif)

### Coordinator crash and recovery

The coordinator crashes after deciding but before collecting acknowledgements.
After recovery it restores its decision from durable state, retransmits,
and completes the protocol:

![Coordinator crash and recovery](examples/coordinator-crash.gif)

This demonstrates the well-known "blocking" of simple 2PC:
participants cannot make progress until a crashed coordinator recovers.

Run the examples:

```sh
cargo run --example timeline     # text timelines for all scenarios
cargo run --example visualize    # interactive HTML trace (opens in browser)
```

## Design

The simulation design is inspired by the "theater of state machines"
model described in [Sled's simulation guide][sled-sim] and
[Polar Signals' DST writeup][polar-dst]: protocol actors are state
machines that communicate exclusively through message passing, driven
by a central simulator that acts as a message bus and scheduler.

[sled-sim]: https://sled.rs/simulation.html
[polar-dst]: https://www.polarsignals.com/blog/posts/2025/07/08/dst-rust

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
