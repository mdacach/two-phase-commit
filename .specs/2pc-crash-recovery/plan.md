# Implementation Plan: 2PC Crash Recovery

**Spec:** .specs/2pc-crash-recovery/spec.md
**Prior art:** .specs/2pc-rust (no-failure baseline)
**Created:** 2026-03-10
**Status:** Implemented
**Estimated Phases:** 3

## Overview

Extend the no-failure 2PC implementation with crash recovery. Build in phases
so each one is independently testable: retransmission first, then crash
injection, then durable state.

---

## Phase 1: Retransmission & Acknowledgement

**Goal:** Add protocol machinery for retransmission and acks, without crashes.
**Status:** Implemented.
**Checkpoint:** Existing `test_safety` and `test_termination` still pass.

### Tasks

- [x] Add `MessageType::Ack`
- [x] Add `AwaitingAcks` phase to coordinator between `Decided` and `Done`
- [x] Track acks in `BTreeSet<NodeId>`; transition to `Done` when all acked
- [x] Embed `last_prepare_time` in `Voting` phase variant
- [x] Embed `last_decision_time` in `AwaitingAcks` phase variant
- [x] Retransmit Prepare on tick when timeout elapsed (to nodes with no vote)
- [x] Retransmit Decision on tick when timeout elapsed (to unacked nodes)
- [x] Participant sends `Ack` on receiving Decision
- [x] Participant re-sends vote on duplicate Prepare (idempotent)
- [x] Participant re-sends Ack on duplicate Decision (idempotent)
- [x] Add `recover()` and `is_quiescent()` to `StateMachine` trait

### Risks

- Retransmission adds redundant messages; ensure property checks still pass.

---

## Phase 2: Crash/Recover in Simulator

**Goal:** Inject crashes and recoveries into the simulation.
**Status:** Implemented.
**Checkpoint:** Safety holds with crashes; termination holds with recovery sweep.

### Tasks

- [x] Add `alive: BTreeMap<ActorId, bool>` to Simulator
- [x] Add `ExternalEvent::Crash(ActorId)` and `ExternalEvent::Recover(ActorId)`
- [x] Drop messages to dead actors, log as `LogEntry::Drop`
- [x] Skip dead actors in `tick`/`TickAll`
- [x] Call `actor.recover()` on `Recover` event
- [x] proptest strategy for `Crash`/`Recover` events
- [x] `test_safety_with_crashes` property test
- [x] `test_termination_with_crashes` with recovery sweep

### Risks

- Crash timing relative to WAL writes matters — Phase 3 must be correct first.

---

## Phase 3: Durable State (WAL)

**Goal:** Persist critical state so actors can recover after crashes.
**Status:** Implemented.
**Checkpoint:** Full proptest with crashes passes.

### Tasks

- [x] Coordinator `Wal { decision: Option<Decision> }`
- [x] Write decision to WAL before sending Decision
- [x] Coordinator `recover()`: restore from WAL, backdate retransmit timestamp
- [x] Participant `Wal { vote: Option<Decision>, decision: Option<Decision> }`
- [x] Write vote to WAL before sending vote
- [x] Write decision to WAL before sending Ack
- [x] Participant `recover()`: restore phase from WAL

### Risks

- Recovery must clear volatile state (votes, acks) but not WAL.

---

## Dependencies Between Phases

```
Phase 1 ──→ Phase 2 ──→ Phase 3
```

Phase 1 (retransmission) is a prerequisite for crash recovery to work.
Phase 2 (crash injection) and Phase 3 (WAL) were developed together since
crash semantics depend on what's durable.
