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

## Crash Recovery Reviews

### coordinator.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 21 | 74 | Move recorded votes and acks into Wal | **Disagree.** Votes and acks are volatile state that is intentionally lost on crash — the coordinator re-collects them via retransmission. Moving them into the WAL would change the recovery semantics: the coordinator would "remember" votes/acks from before the crash, which is incorrect. The WAL should only contain the durable decision. The current separation (WAL = durable, struct fields = volatile) correctly models the crash boundary. | Skipped |
| 22 | 84 | Move last_prepare_time and last_decision_time to relevant CoordinatorPhase | **Agree.** These timestamps are phase-specific: `last_prepare_time` is only meaningful in `Voting`, `last_decision_time` only in `AwaitingAcks`. Embedding them in the enum variants eliminates stale `Option` values and makes illegal states unrepresentable. `Voting { last_prepare_time: u64 }` and `AwaitingAcks { decision: Decision, last_decision_time: u64 }`. | Fixed |
| 23 | 243 | Do we need the prob > 0 check? | **Agree — it's redundant.** `random_bool(0.0)` always returns false (Bernoulli with p_int=0). The `prob > 0.0` guard is pure optimization/documentation. Removing it simplifies without changing behavior. | Fixed |
| 24 | 290 | Log if ignoring in tick catch-all | **Disagree.** Unlike `on_message`'s catch-all (which fires on genuinely unexpected messages), the tick catch-all only covers `Waiting` and `Done` — normal quiescent states hit by every `TickAll`. Logging here would be extremely noisy without diagnostic value. | Skipped |
| 25 | 295 | Add documentation to is_quiescent | **Agree.** Trait method has a doc comment but the impl should clarify the coordinator-specific semantics. | Fixed |
| 26 | 303 | Add documentation to recover | **Agree.** The module-level doc covers recovery, but the method itself deserves a summary and explanation of the `saturating_sub` trick. | Fixed |
| 27 | 309 | Why this specific last_decision_time? | Documentation issue — answered in the doc comment for `recover()`. The value `at_time - retransmit_timeout` ensures the retransmit check (`elapsed >= timeout`) passes on the very next tick, triggering immediate retransmission after recovery. | Fixed |
| 28 | 324 | Move tests to another file | **Agree.** Unit tests for coordinator and participant have grown large enough to warrant separate files. Moves tests to `coordinator/tests.rs` and `participant/tests.rs` while keeping them in `mod tests` (same visibility). | Fixed |

### participant.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 29 | 164 | Explain why duplicate Prepare re-send is needed | **Agree.** The comment says "Duplicate Prepare while already voted: re-send vote" but doesn't explain *why*. After a coordinator crash and recovery (WAL has no decision), it re-enters Voting with cleared votes and retransmits Prepare. Participants that already voted receive a duplicate Prepare and must re-send their vote so the coordinator can re-collect votes and decide. | Fixed |
| 30 | 188 | Similarly, explain why duplicate Decision re-send Ack is needed | **Agree.** After coordinator crash and recovery (WAL has decision), it re-enters Decided, transitions to AwaitingAcks, and retransmits Decision. Participants that already decided receive a duplicate Decision and must re-send Ack so the coordinator can complete the protocol. | Fixed |
| 31 | 226 | Move tests to another file | **Agree.** Same as #28. | Fixed |

### state_machine.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 32 | 23 | Consider whether recover() and is_quiescent() belong in StateMachine | **Agree — they belong here.** Both are universal actor concerns: every actor needs crash recovery semantics, and every actor has a quiescence state. The alternatives (separate traits, downcasting, or simulator-level dispatch) all add complexity without benefit. The trait already has `tick` which is similarly simulator-oriented. The default implementations (no-op recover, non-quiescent) are safe conservative defaults. Removing the REVIEW comment is sufficient. | Fixed |

### lib.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 33 | 30 | Update documentation to reflect Alloy specification | **Agree.** The crate doc only mentions TLA+, but there's a complete Alloy spec at `alloy/TwoPhaseCommit.als`. Should add a parallel correspondence table and note the Alloy model's distinguishing features (ever-growing message network, nondeterministic coordinator abort, weak fairness on vote reception). | Fixed |

## Round 3

### participant.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 34 | 31 | Separate `Decision` into `Vote` and `Decision` for less ambiguity | **Disagree.** `Vote` and `Decision` are isomorphic (`Commit`/`Abort`). A separate `Vote` type would require conversion at every boundary (`check_validity` compares votes to decisions, coordinator stores votes as `Decision`, WAL uses `Decision` for both). The naming in context (`vote: Option<Decision>` vs `decision: Decision`) already disambiguates. The added type-level boilerplate wouldn't catch real bugs. | Skipped |
| 35 | 51 | Rename "wal" to `durable_state` | **Agree.** The struct is not a write-ahead log (there's no log); it's durable state that survives crashes. `DurableState` is more accurate. Renamed struct, field, and all references in both coordinator and participant. | Fixed |

### coordinator.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 36 | 17 | Make doc links for phases, messages, and structures in module docs | **Agree.** Converted plain-text references to rustdoc links (`[CoordinatorPhase::Voting]`, `[MessageType::Prepare]`, `[StateMachine::tick]`, etc.) so the docs are navigable. | Fixed |

### simulator/event.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 37 | 56 | Better comment on `impl Ord` | **Agree.** Replaced with a doc comment explaining natural-order-then-reverse for min-heap semantics. | Fixed |
| 38 | 60 | Use `.reverse()` instead of `other.cmp(self)` | **Agree.** `self.cmp(other).reverse()` reads as "natural order, reversed" — clearer than the swapped-arguments idiom. | Fixed |
| 39 | 74–79 | `PartialEq` ignores event payload; include all fields | **Agree.** The custom `PartialEq` was safe only because sequence numbers are unique, but it was fragile and violated the spirit of the `Ord`/`Eq` consistency contract. Added `Ord`/`PartialOrd` derives to `Event`, `ExternalEvent`, `InternalEvent`, `Message`, and `MessageType`, then included the event payload in `TimestampedEvent`'s `Ord` and removed the custom `PartialEq`/`Eq` (now derived). | Fixed |
| 40 | 90 | Explain use of sequence number (field) | **Agree.** Added doc comment: monotonically increasing counter that breaks timestamp ties, preserving FIFO insertion order for deterministic runs. | Fixed |
| 41 | 103 | Explain use of sequence number (in `insert`) | **Agree.** Added inline comment at assignment site. | Fixed |

### lib.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 42 | 18 | Convert crash-recovery features table to bullet list | **Agree.** Tables are hard to scan for prose descriptions. Converted to a bullet list with bold feature names and short explanations. Also updated "Write-ahead log" → "Durable state" to match the rename. | Fixed |

## Round 4

### coordinator.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 43 | 1 | Use ascii-diagram skill for phase transitions | **Already addressed.** Diagrams were present but used a compact single-line layout. Replaced with a vertical box-drawing diagram that clearly shows both paths to `Decided` (all votes in + spontaneous abort). Removed REVIEW marker. | Fixed |
| 44 | 257 | Log duplicate ack in Done state | **Agree.** Added `trace!` log. Distinguishes expected late duplicates from the `warn!`-level catch-all for genuinely unexpected messages. | Fixed |

### participant.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 45 | 1 | Use ascii-diagram skill for phase transitions | **Already addressed.** Replaced compact text diagram with a two-column box-drawing layout showing both paths to `Decided` (via `Voted` with vote retained, or directly from `Waiting` with vote lost). Removed REVIEW marker. | Fixed |

### simulator.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 46 | 60 | Make enum for Operating/Crashed | **Agree.** Replaced `BTreeMap<ActorId, bool>` with `BTreeMap<ActorId, ActorStatus>` where `ActorStatus` is `Operating` or `Crashed`. Self-documenting at every use site. Skipped crash timestamp as noted unused by the reviewer. | Fixed |

### properties.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 47 | 1–6 | Significant changes needed; move property checks to simulator; simulator should track votes | **Stale.** The concerns were addressed in rounds 1–2: `check_validity` now checks both coordinator vote records (when available) and participant vote records (always available) as ground truth. The simulator already calls `check_all_invariants` after every step. Removed the 6-line REVIEW block. | Removed |
| 48 | 57 | Empty `// REVIEW:` marker | **Stale.** Leftover from a previous round. Removed. | Removed |
| 49 | 64–66 | Simulator must track votes so commit-validity survives coordinator crash | **Agree.** Added `observed_votes: BTreeMap<NodeId, Vote>` to `Simulator`, recorded in `enqueue_outgoing` when VoteCommit/VoteAbort messages are sent. `check_validity` now takes `observed_votes` from the simulator instead of reading the coordinator's volatile vote map. The manual test in `tests/simulation.rs` builds observed votes from the known input. | Fixed |

## Round 5

### coordinator.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 50 | 120 | Move votes and acks to inside the coordinator phase | **Agree.** Now that the simulator tracks wire-level observations for property checking, the coordinator's votes and acks no longer need to be externally accessible. Moved `votes: BTreeMap<NodeId, Vote>` into `Voting` and `acks: BTreeSet<NodeId>` into `AwaitingAcks`. Removed `Copy` derive from `CoordinatorPhase` (collections aren't `Copy`); `phase()` now returns `&CoordinatorPhase`. Removed the `votes()` public accessor. The `on_message` match was restructured from `match (msg_type, self.phase)` (which relied on `Copy`) to `match msg.message_type` with `matches!()` guards. `tick()` uses sequential `if let` blocks with in-place timestamp updates (`*last_prepare_time = at_time`) instead of reconstructing the entire phase variant. `recover()` is cleaner — no explicit `clear()` calls needed since the old phase (with its collections) is simply dropped on reassignment. | Fixed |

### participant.rs

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 51 | 235 | Explain why tick is no-op | **Agree.** Added doc comment: the participant is purely reactive — it has no retransmission timer and no spontaneous actions, producing messages only in response to Prepare or Decision from the coordinator. | Fixed |

### participant/tests.rs, examples, README

| # | Line | Review | Assessment | Status |
|---|------|--------|------------|--------|
| 52 | tests:3 | Update remaining "WAL" references to "durable state" | **Agree.** Review #35 renamed the `Wal` struct to `DurableState` but left stale "WAL" wording in comments and prose. Updated: `participant/tests.rs` (2 comments), `examples/scenarios/mod.rs` (2 doc comments), `README.md` (1 paragraph). Spec and plan files left unchanged as historical records. | Fixed |
