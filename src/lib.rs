//! A simulation of the Two-Phase Commit (2PC) protocol, developed alongside a
//! TLA+ specification (`tla/TwoPhaseCommit.tla`) and an Alloy specification
//! (`alloy/TwoPhaseCommit.als`).
//!
//! # Architecture
//!
//! The protocol actors (coordinator, participants) are implemented as
//! [`StateMachine`](state_machine::StateMachine)s that consume messages and
//! produce outgoing messages.  A discrete-event [`Simulator`](simulator::Simulator)
//! drives them through an event queue with configurable delivery delay, checking
//! [safety invariants](simulator::properties) after every step.
//!
//! # Extensions beyond the formal specs (crash recovery)
//!
//! The Rust implementation extends the no-failure model with crash recovery
//! mechanics that have no TLA+ or Alloy counterpart:
//!
//! - **Ack message** — participants acknowledge the decision, letting the
//!   coordinator know delivery succeeded.
//! - **[`AwaitingAcks`](coordinator::CoordinatorPhase::AwaitingAcks) phase** —
//!   the coordinator waits for all Acks before completing.
//! - **Durable state** — both coordinator and participant persist critical
//!   state to survive crashes (`DurableState` structs).
//! - **Retransmission** — the coordinator retransmits Prepare or Decision
//!   messages on [`tick`](state_machine::StateMachine::tick) after a timeout.
//! - **Idempotent re-send** — participants re-send their vote or Ack when
//!   they receive a duplicate Prepare or Decision.
//! - **Crash / Recover** — the [`Simulator`](simulator::Simulator) can crash
//!   and [`recover`](state_machine::StateMachine::recover) actors mid-protocol.
//!
//! These extensions preserve Agreement (AC1) and Validity (AC2). Termination
//! requires eventual recovery of all crashed actors.

pub mod coordinator;
pub mod participant;
pub mod simulator;
pub mod state_machine;
pub mod types;
