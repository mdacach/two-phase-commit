# Implementation Plan: 2pc-alloy

**Spec:** `.specs/2pc-alloy/spec.md`
**Created:** 2026-03-04
**Estimated Phases:** 5

## Context

We have a working TLA+ spec for two-phase commit (`tla/TwoPhaseCommit.tla`) and want an
idiomatic Alloy 6 model of the same protocol. The scratch.als in the alloy/ directory shows
established patterns for temporal modeling and event reification that we should follow.

## Overview

Build an Alloy 6 specification in `alloy/TwoPhaseCommit.als` using temporal operators,
`var sig` subsetting for participant/coordinator state, and an ever-growing message set for
the network. The model covers one transaction with no failures. Properties (Agreement,
Validity, Termination) are checked as assertions. Fairness is encoded explicitly with
temporal operators. Events are reified for GUI visibility.

---

## Phase 1: Core Model — Sigs, State, Initial Conditions
**Goal:** Define the static structure and mutable state; verify it parses.
**Checkpoint:** `alloy exec` parses without errors.

### Tasks
- [ ] Create `alloy/TwoPhaseCommit.als` with sig declarations:
  - `sig Node {}` for participants
  - Message type enum/abstract sig (Prepare, VoteCommit, VoteAbort, DecCommit, DecAbort)
  - `sig Msg { mtype, dest }` with `var sig Sent in Msg` for the ever-growing network
  - Coordinator mutable state via var sigs (phase tracking, votes received)
  - Participant mutable state via var sig subsetting (HasVoted, VotedCommit, ParticipantDecided, ParticipantCommitted)
- [ ] Define initial state fact (no messages sent, no votes, no decisions)
- [ ] Add monotonicity fact: `always Sent in Sent'`
- [ ] Run `alloy exec` to verify parsing

### Files to Create/Modify
- `alloy/TwoPhaseCommit.als` — create

### Risks
- Modeling coordinator phase cleanly without a dedicated sig may require iteration

---

## Phase 2: Event Predicates
**Goal:** Define all protocol actions as predicates with full frame conditions.
**Checkpoint:** `run` produces a sensible example trace.

### Tasks
- [ ] Implement event predicates:
  1. `coordinatorSendPrepare` — adds Prepare messages for all Nodes to Sent
  2. `participantVote[n]` — nondeterministically votes commit or abort; adds vote msg
  3. `coordinatorReceiveVote[n]` — records vote from participant n
  4. `coordinatorDecide` — decides commit (all voted commit) or abort
  5. `coordinatorSendDecision` — adds decision messages to Sent
  6. `participantReceiveDecision[n]` — records decision for participant n
  7. `stutter` — nothing changes
- [ ] Add transition fact: `always (action1 or action2 or ... or stutter)`
- [ ] Add a basic `run example {}` command
- [ ] Run `alloy exec` — inspect trace for sensible behavior

### Files to Create/Modify
- `alloy/TwoPhaseCommit.als` — modify

### Risks
- Frame conditions are the most error-prone part; every `var` must be constrained in every
  predicate. Verify by running examples and checking for spurious state changes.

---

## Phase 3: Properties, Fairness, and Assertions
**Goal:** Encode Agreement, Validity, Termination; add fairness.
**Checkpoint:** Safety properties pass; Termination passes under fairness.

### Tasks
- [ ] Add Agreement assertion: `always` all decided participants agree
- [ ] Add Validity assertion: `always` commit decision implies all voted commit
- [ ] Add fairness fact using `always (enabled implies eventually taken)` pattern
- [ ] Add Termination assertion: `eventually` all participants decided
- [ ] Run `alloy exec` for each check; debug counterexamples if any

### Files to Create/Modify
- `alloy/TwoPhaseCommit.als` — modify

### Risks
- Fairness encoding is subtle; may need iteration to get the right temporal pattern

---

## Phase 4: Event Reification and Multi-Scope Checks
**Goal:** Add GUI-visible events; check at scopes 2, 3, 4.
**Checkpoint:** All checks pass at all scopes; events visible in GUI.

### Tasks
- [ ] Add `enum Event` and reification functions (following scratch.als pattern)
- [ ] Add `run` and `check` commands for 2, 3, 4 Node scopes
- [ ] Run all checks via `alloy exec`; confirm all pass

### Files to Create/Modify
- `alloy/TwoPhaseCommit.als` — modify

---

## Phase 5: Failure Mode Research Report
**Goal:** Deliver a report on what changes are needed to model failures.
**Checkpoint:** Report written.

### Tasks
- [ ] Research 2PC failure modes (coordinator crash, participant crash, network partition)
- [ ] For each, describe what Alloy modeling changes are needed
- [ ] Write report to `.specs/2pc-alloy/failure-modes.md`

### Files to Create/Modify
- `.specs/2pc-alloy/failure-modes.md` — create

---

## Dependencies Between Phases
Phase 1 → Phase 2 → Phase 3 → Phase 4 (sequential)
Phase 5 can start after Phase 3 (independent of Phase 4)

## Risk Areas
1. **Frame conditions** (Phase 2): Every `var` must be constrained in every event predicate.
2. **Fairness encoding** (Phase 3): Getting "enabled ⟹ eventually taken" right in Alloy temporal logic.
3. **Scope vs. trace length**: Large scopes may need `1.. steps` for unbounded traces.

## Open Items
- None.
