# Implementation Plan: 2PC No Failures

**Spec:** .specs/2pc-no-failures/spec.md
**Created:** 2026-02-28
**Estimated Phases:** 3

## Overview

Build a raw TLA+ specification of two-phase commit (no failures) in three
incremental phases. Phase 1 defines the constants, variables, message types,
and the Init/Next state machine skeleton. Phase 2 adds the coordinator and
participant actions that implement the full protocol. Phase 3 adds safety
invariants, the consistency invariant, the liveness property, a TLC
configuration file, and runs TLC to verify everything passes.

Each phase ends with a TLC checkpoint so errors are caught early.

---

## Phase 1: Skeleton — Constants, Variables, Init, Helpers

**Goal:** A valid TLA+ module that TLC can parse and check (trivially) with
`Init` and a stuttering `Next`.

**Checkpoint:** TLC accepts the spec with no parse or evaluation errors.

### Tasks
- [x] Create `tla/TwoPhaseCommit.tla` with module header and `EXTENDS`
- [x] Declare CONSTANT `Participants`
- [x] Declare VARIABLES: `messages`, `coordinator_phase`, `votes`, `participant_phase`, `decision`
- [x] Define message-type constants (`PREPARE`, `VOTE_COMMIT`, `VOTE_ABORT`, `COMMIT`, `ABORT`)
- [x] Define `Init`: initialize all variables (empty message set, phases to "waiting", votes/decisions to "none")
- [x] Define placeholder `Next == UNCHANGED vars`
- [x] Define `vars` tuple and `Spec == Init /\ [][Next]_vars`
- [x] Create `tla/TwoPhaseCommit.cfg` with CONSTANT assignment (e.g., `Participants = {p1, p2}`) and `SPECIFICATION Spec`
- [x] Run TLC to confirm no errors

### Files to Create
- `tla/TwoPhaseCommit.tla` — the spec
- `tla/TwoPhaseCommit.cfg` — TLC config

### Risks
- Getting the record/function types right on first pass. Mitigated by running TLC immediately.

---

## Phase 2: Protocol Actions

**Goal:** Full protocol behavior: coordinator sends PREPAREs, participants
vote, coordinator decides (with early abort), participants persist the
decision.

**Checkpoint:** TLC explores states without errors; spot-check that traces
show the expected protocol flow.

### Tasks
- [x] `CoordinatorSendPrepare`: guarded by `coordinator_phase = "waiting"`, adds a `PREPARE` message per participant to `messages`, advances phase to `"voting"`
- [x] `ParticipantVote(p)`: guarded by participant `p` in `"waiting"` phase and a matching `PREPARE` in `messages`, nondeterministically picks `VOTE_COMMIT` or `VOTE_ABORT`, adds vote message, advances participant phase to `"voted"`
- [x] `CoordinatorReceiveVote(p)`: guarded by coordinator in `"voting"` phase, reads a vote message from `p` in `messages`, records it in `votes`; if vote is `VOTE_ABORT`, immediately transitions coordinator to `"decided"` with `ABORT`
- [x] `CoordinatorDecide`: guarded by coordinator having received all votes and all are `VOTE_COMMIT`, transitions to `"decided"` with `COMMIT`
- [x] `CoordinatorSendDecision`: guarded by coordinator in `"decided"` phase, sends `COMMIT` or `ABORT` to all participants, advances phase to `"done"`
- [x] `ParticipantReceiveDecision(p)`: guarded by matching decision message in `messages`, participant persists `decision[p]`, advances to `"decided"`
- [x] Wire all actions into `Next` as a disjunction
- [x] Run TLC to confirm no errors

### Files to Modify
- `tla/TwoPhaseCommit.tla` — add actions, update `Next`

### Risks
- Coordinator early-abort logic interacting with votes that arrive after the decision. Guard conditions must prevent processing votes once decided.
- Enabling condition gaps causing deadlock. TLC will surface these as liveness failures in Phase 3.

---

## Phase 3: Properties & Verification

**Goal:** Define and verify safety invariants, consistency invariant, and
liveness property. All acceptance criteria pass under TLC.

**Checkpoint:** TLC reports all invariants and temporal properties satisfied.

### Tasks
- [x] Define `Agreement`: `\A p1, p2 \in Participants: decision[p1] /= "none" /\ decision[p2] /= "none" => decision[p1] = decision[p2]`
- [x] Define `Consistency`: if coordinator decided `COMMIT`, then `\A p \in Participants: votes[p] = "VOTE_COMMIT"`
- [x] Define `Termination`: `<>(\A p \in Participants: decision[p] /= "none")`
- [x] Add `INVARIANT Agreement` and `INVARIANT Consistency` to `.cfg`
- [x] Add `PROPERTY Termination` to `.cfg`; add fairness condition to `Spec` (`WF_vars(Next)`)
- [x] Run TLC — no counterexamples
- [x] Verify with a single participant (`Participants = {p1}`) — 11 states, pass
- [x] Verify with three participants (`Participants = {p1, p2, p3}`) — 506 states, pass

### Files to Modify
- `tla/TwoPhaseCommit.tla` — add invariants, liveness property, fairness
- `tla/TwoPhaseCommit.cfg` — add INVARIANT and PROPERTY lines

### Risks
- Fairness granularity: `WF_vars(Next)` may be too weak if individual actions need to be independently fair. May need per-action weak fairness (`WF_vars(CoordinatorSendPrepare) /\ WF_vars(ParticipantVote(p)) /\ ...`).
- State space explosion with 3 participants. Should be manageable without failures, but watch TLC runtime.

---

## Dependencies Between Phases
Phase 1 → Phase 2 → Phase 3 (strictly sequential)

## Risk Areas
- **Early abort + lingering votes:** The coordinator must ignore vote messages once it has decided. Getting this guard wrong is the most likely source of invariant violations.
- **Fairness for liveness:** Choosing the right fairness conditions is critical for the termination property. Too weak = false counterexamples; too strong = hiding real deadlocks.

## Open Items
None.
