//! A simulation of the Two-Phase Commit (2PC) protocol, developed alongside a
//! TLA+ specification (`tla/TwoPhaseCommit.tla`).
//!
//! # Architecture
//!
//! The protocol actors (coordinator, participants) are implemented as
//! [`StateMachine`](state_machine::StateMachine)s that consume messages and
//! produce outgoing messages.  A discrete-event [`Simulator`](simulator::Simulator)
//! drives them through an event queue with configurable delivery delay, checking
//! [safety invariants](properties) after every step.
//!
//! # Relationship to the TLA+ spec
//!
//! The Rust implementation is a *strict superset* of the TLA+ model:
//!
//! | TLA+ action               | Rust code path                                    | Notes |
//! |---------------------------|---------------------------------------------------|-------|
//! | `CoordinatorSendPrepare`  | `Coordinator::on_message(StartTransaction, …)`    | Exact match |
//! | `CoordinatorReceiveVote`  | `Coordinator::on_message(VoteCommit/Abort, …)`    | Exact match |
//! | `CoordinatorDecide`       | `Coordinator::try_decide`                         | Extended: `abort_bias` can flip a unanimous commit to abort |
//! | `CoordinatorSendDecision` | `Coordinator::try_send_decision`                  | Called from both `on_message` and `tick` |
//! | `ParticipantVote`         | `Participant::on_message(Prepare, …)`             | TLA+ is nondeterministic; Rust uses `abort_bias` probability |
//! | `ParticipantReceiveDecision` | `Participant::on_message(DecisionCommit/Abort, …)` | Exact match |
//! | *(none)*                  | `Coordinator::tick` spontaneous abort              | Extension: models coordinator timeout/crash |
//!
//! The TLA+ `Consistency` invariant corresponds to [`properties::check_validity`]
//! (commit direction). The TLA+ `Agreement` invariant corresponds to
//! [`properties::check_agreement`].

// REVIEW: Update documentation to reflect Alloy's specification too.

pub mod coordinator;
pub mod participant;
pub mod properties;
pub mod simulator;
pub mod state_machine;
pub mod types;
