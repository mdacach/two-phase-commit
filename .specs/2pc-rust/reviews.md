# 2pc-rust Review Tracker

## coordinator.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 1 | 10 | Add votes to phases Voting, Decided, Done | **Disagree.** Votes accumulate over the coordinator's lifetime and persist across phase transitions. Embedding them in the enum loses `Copy`, forces destructure+reconstruct on every phase change, and makes `try_decide` (which mutates votes during Voting) awkward. The phase governs what operations are legal; votes are accumulated data — different concerns. | Skipped |
| 2 | 19 | Put rng and abort_bias under a single "config" field | Agree. Groups simulation-config together, separates it from protocol state. | Fixed |
| 3 | 76 | Change match to (message, phase) | Agree. Eliminates `if matches!` guards; patterns are more explicit. | Fixed |
| 4 | 78 | Log if ignoring message | Agree. Helps with debugging failed property tests. | Fixed |
| 5 | 108 | If decided at this moment, already send messages | Agree. Removes the need for a separate tick to broadcast after vote-triggered decisions. `try_send_decision` helper handles both vote-triggered and tick-triggered paths. | Fixed |
| 6 | 167 | Test name not true with abort_bias | Agree. Renamed to clarify it tests the zero-abort-bias path. | Fixed |

## participant.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 7 | 21 | Make predetermined vote similar to abort_bias | Agree. Replace `Option<Decision>` with `abort_bias: f64`. `with_fixed_vote` becomes sugar for `abort_bias=0.0` or `1.0`. | Fixed |
| 8 | 22 | Batch predetermined vote and rng into "config" field | Agree. Same pattern as coordinator. | Fixed |
| 9 | 78 | Match on (message, phase) | Agree. Same rationale as coordinator. | Fixed |
| 10 | 79 | Log phase and message if ignoring | Agree. | Fixed |

## simulator.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 11 | 23 | Make delivery delay be a Rust range | Agree. `Range<u64>` is more expressive than a single upper bound. | Fixed |
| 12 | 75 | Remove implicit tick before delivering a message | Agree. Tick responsibility moved into coordinator's `on_message`. Eliminates hidden coupling between simulator and state machine internals. | Fixed |
| 13 | 113 | Use imports so types are shorter | Agree. | Fixed |
| 14 | 148 | Better error message | Agree. Now surfaces the invariant error string instead of a generic message. | Fixed |

## properties.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 15 | 17 | Compute committed/aborted node sets for better error | Agree. Shows all committed and all aborted nodes in the error. | Fixed |
| 16 | 31 | Investigate 2PC properties — is this one accurate? | Investigated. The check is accurate: it matches the standard "commit-validity" property. See assessment below. | Fixed |
| 17 | 33 | Check if abort decision from coordinator was respected | Agree. Merged into a single `check_validity` that covers both directions. | Fixed |
| 18 | 56 | Merge commit and abort validity checks | Agree. Single `check_validity` function now checks both commit-validity and abort-validity. | Fixed |

### Property investigation (#16) assessment

Standard 2PC safety properties (Babaoglu-Toueg AC1-AC4 / Guerraoui NBAC1-NBAC4):

1. **Agreement (AC1)**: All participants that decide reach the same decision.
2. **Commit-Validity (AC2/NBAC3)**: Commit can only be decided if all participants voted yes.
3. **Abort-Validity (NBAC4)**: Abort can only be decided if some participant voted no or crashed.

Our `check_validity_commit` is AC2 — accurate and standard. The original `check_validity_abort`
checked "coordinator aborted → no participant committed," which is actually a consequence of
Agreement, not true abort-validity. However, true abort-validity ("coordinator aborted → some
participant voted no") is wrong for our model because the coordinator can legitimately abort via
`abort_bias` even with unanimous commits. So our weaker check is correct for this model.

Both validity directions are now merged into a single `check_validity` function.

Sources: Babaoglu & Toueg (1993), Gray & Lamport (2006), Guerraoui EPFL lectures.

## tests/simulation.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 19 | 14 | StartTransaction must only happen once, at start | Agree. Restructured strategy: always starts with StartTransaction at time 0, then generates only TickAll events. | Fixed |
| 20 | 43 | Investigate quiescence modeling | Investigated. The tick-tail approach is standard practice for bounded simulation. Added a `drain` method as a cleaner alternative. See assessment below. | Fixed |

### Quiescence investigation (#20) assessment

Approaches found in practice:
- **FoundationDB**: Bounded simulated time, no quiescence detection, invariants checked throughout.
- **Jepsen/Maelstrom**: Bounded workload duration, explicit `:drain` operations for queues.
- **Stateright**: Full state-space exploration; quiescence = no enabled actions (TLA+ stuttering).
- **Sled simulation guide**: Event loop until queue empty; no formal quiescence probe.
- **Academic (Dijkstra-Safra)**: "All processes passive AND all channels empty."

The tick-tail approach is the standard pattern for bounded simulators. Implemented `drain(max_rounds)`
on Simulator: repeatedly processes events, then injects TickAll probes (routed through `step()` for
invariant checking) until no actor produces new messages. This replaces hardcoded tick tails with a
principled quiescence check while maintaining a bounded iteration safety valve.

A future improvement could add `is_quiescent()` to the StateMachine trait (coordinator: quiescent
in Waiting/Done; participants: always quiescent) — the runtime analog of TLA+ stuttering.

Sources: FoundationDB paper (SIGMOD 2021), Polar Signals DST blog, Dijkstra-Safra termination
detection, Segala (quiescence/fairness/testing).
