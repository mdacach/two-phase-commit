# Two-Phase Commit: Formal Specification and Simulation

A study of the Two-Phase Commit (2PC) protocol, developed across three
formalisms:

1. **TLA+** (`tla/TwoPhaseCommit.tla`) — safety and liveness model-checked
   with TLC.
2. **Alloy 6** (`alloy/TwoPhaseCommit.als`) — bounded verification with
   temporal operators and event reification.
3. **Rust** (`src/`) — deterministic discrete-event simulation with crash
   recovery, property-based testing via proptest.

Each formalism verifies the same core properties:

- **Agreement (AC1):** All decided participants reach the same decision.
- **Validity (AC2):** Commit requires unanimous commit votes.
- **Termination:** Under fair scheduling (and eventual recovery), all
  participants eventually decide.

## Building and testing

```sh
cargo test                     # unit tests + property-based tests (proptest)
cargo run --example timeline   # print protocol timelines (normal + crash recovery)
cargo doc --open               # browse API documentation
```

## Project structure

```
tla/                    TLA+ specification (no-failure model)
alloy/                  Alloy 6 specification (no-failure model)
src/
  coordinator.rs        Coordinator state machine (voting -> decision -> acks)
  participant.rs        Participant state machine (vote -> decide)
  simulator.rs          Deterministic event-queue simulator with crash injection
  properties.rs         Safety invariants checked after every simulation step
  state_machine.rs      StateMachine trait (on_message, tick, recover)
  types.rs              Core types (NodeId, Message, Decision)
tests/simulation.rs     Property-based tests with proptest
examples/timeline.rs    Human-readable protocol traces
.specs/                 Specification and planning documents
```

## Design overview

Actors implement a `StateMachine` trait with `on_message` (react to messages)
and `tick` (time-driven actions like retransmission and spontaneous abort).
The `Simulator` drives them through a min-heap event queue with configurable
delivery delay and crash injection. Safety invariants are checked after every
event.

Crash recovery uses write-ahead logs (WAL): the coordinator persists its
decision before broadcasting; participants persist their vote before
responding. On recovery, actors restore from WAL and retransmit.
