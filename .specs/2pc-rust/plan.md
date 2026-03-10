# Implementation Plan: 2PC in Rust (Simulation)

**Spec:** .specs/2pc-rust/spec.md
**Reference:** alloy/TwoPhaseCommit.als
**Prior art:** ../reliable-channel (StateMachine trait, simulator architecture)
**Created:** 2026-03-09
**Estimated Phases:** 4

## Overview

Implement the two-phase commit protocol as per-actor state machines in Rust, communicating
through message passing. A `StateMachine` trait defines two methods: `on_message` (react to
incoming messages) and `tick` (take self-initiated actions). The coordinator and each
participant implement this trait independently.

A deterministic simulation harness — following the same event-queue architecture as
`../reliable-channel` — manages external events (test orchestration) and internal events
(protocol messages) in a single min-heap ordered by `(timestamp, sequence_number)`. The
coordinator initiates the protocol in response to a `StartTransaction` external event. When
delivering a message, the simulator calls `tick()` on the recipient first (implicit tick),
then `on_message()`. Delivery timestamps are generated via the simulator's RNG, producing
reproducible but varied interleavings across seeds.

Three sources of nondeterminism mirror the Alloy spec: message delivery order (RNG-based
jitter), participant vote choice (per-participant RNG), and coordinator abort bias
(configurable probability of aborting even when all votes are commit).

No async, no real networking — this is a simulation-only implementation designed to validate
the protocol logic.

---

## Actor State Machine Details

### Coordinator

**States:** `Waiting → Voting → Decided → Done`

**Fields:**
```rust
struct Coordinator {
    nodes: Vec<NodeId>,                       // participant set (immutable after construction)
    phase: Phase,                             // starts at Waiting
    decision: Option<Decision>,               // None until decided
    votes: BTreeMap<NodeId, Decision>,        // recorded votes (node → commit/abort)
    rng: ChaCha8Rng,                          // for abort bias decisions
    abort_bias: f64,                          // probability of aborting despite all commits
}
```

**`on_message` behavior:**

| Message          | Guard                                       | Effect                                        |
|------------------|---------------------------------------------|-----------------------------------------------|
| StartTransaction | phase == Waiting                            | Send `Prepare` to all nodes; phase → Voting   |
| VoteCommit(n)    | phase == Voting, n ∉ votes                  | Record votes[n] = Commit; try_decide()        |
| VoteAbort(n)     | phase == Voting, n ∉ votes                  | Record votes[n] = Abort; try_decide()         |
| *anything else*  | —                                           | Ignore (return empty vec)                     |

After recording a vote, `on_message` calls `try_decide()` which checks if the coordinator
should commit or abort based on the current vote tally. This merges `coordinatorReceiveVote`
and `coordinatorDecide` into a single step for the common case (vote triggers decision).
Spontaneous aborts still happen in `tick()`.

`try_decide()` logic:
- If any vote is Abort → decide Abort
- If all votes are Commit (votes.len() == nodes.len()) → decide Commit with probability
  `1 - abort_bias`, otherwise Abort
- Otherwise → no decision yet

**`tick` behavior:**

| Phase   | Condition                                                 | Effect                                               |
|---------|-----------------------------------------------------------|------------------------------------------------------|
| Waiting | —                                                         | No-op (waits for StartTransaction via on_message)    |
| Voting  | —                                                         | Spontaneous Abort with prob `abort_bias / 10`; phase → Decided. Otherwise: no-op |
| Decided | —                                                         | Send DecCommit/DecAbort to all nodes; phase → Done   |
| Done    | —                                                         | No-op                                                |

The `try_decide()` method (called from `on_message`) handles the standard cases: abort if
any abort vote, commit if all vote commit (with `abort_bias` chance of aborting instead).
The spontaneous abort in `tick()` exercises the Alloy spec's freedom to abort at any point
during voting (the guard for `coordinatorDecide[Abort]` only requires `phase = Voting`).

**Accessor methods:** `phase()`, `decision()`, `votes()`, `nodes()`

### Participant

**States:** Explicit via `ParticipantPhase` enum.

```rust
enum ParticipantPhase {
    Waiting,           // hasn't received Prepare yet
    Voted,             // has voted, awaiting coordinator's decision
    Decided(Decision), // has received and applied coordinator's decision
}
```

**Fields:**
```rust
struct Participant {
    id: NodeId,
    phase: ParticipantPhase,      // lifecycle state
    vote: Option<Decision>,       // what this participant voted (set on Prepare)
    rng: ChaCha8Rng,              // for vote choice
}
```

**`on_message` behavior:**

| Message    | Guard                  | Effect                                                              |
|------------|------------------------|---------------------------------------------------------------------|
| Prepare    | phase == Waiting       | Draw from rng: send VoteCommit or VoteAbort; phase → Voted         |
| DecCommit  | phase == Voted         | phase → Decided(Commit)                                             |
| DecAbort   | phase == Voted         | phase → Decided(Abort)                                              |
| *anything* | —                      | Ignore                                                              |

The participant's vote is drawn from its RNG when Prepare arrives, not at construction
time. This means the same participant with the same seed always votes the same way, but
different seeds produce different votes — matching the Alloy spec's disjunction in
`participantVote`.

**`tick`:** Uses default no-op. Participants are purely reactive.

**Accessor methods:** `phase()`, `vote()`, `decision()`, `has_voted()`

### Vote Probability

Each participant draws `rng.gen_bool(0.8)` to choose commit/abort, giving an 80/20 split
toward commit. With N participants, the probability that all vote commit is `0.8^N` (41% for
N=4), ensuring good coverage of both the commit and abort paths. The proptest seed
determines each participant's RNG seed (derived from the simulator's master seed), so runs
are fully reproducible.

---

## Example Property-Test Trace

A concrete trace from a proptest run with 2 participants, showing the event queue in
action. The proptest strategy generates external events with random time deltas; the
simulator generates internal events (message deliveries) with RNG-based jitter.

```
=== proptest parameters ===
  n_participants: 2
  simulator_seed: 42
  abort_bias: 0.05
  jitter_range: 5
  generated events: [StartTransaction@0, TickAll@+3, TickAll@+2, TickAll@+4, TickAll@+1]
  materialized:     [StartTransaction@0, TickAll@3, TickAll@5, TickAll@9, TickAll@10]
  tail appended:    [TickAll@11, TickAll@12, ..., TickAll@20]

=== simulation ===

Step 1 | t=0 | External(StartTransaction) → Coordinator
  implicit tick: Coordinator [Waiting] → no-op
  on_message(StartTransaction):
    Coordinator: Waiting → Voting
    outgoing: Prepare→Node(0) [enqueued t=4], Prepare→Node(1) [enqueued t=2]
  ✓ Agreement (no one decided)  ✓ Validity

Step 2 | t=2 | Internal: Deliver Prepare → Node(1)
  implicit tick: Node(1) → no-op
  on_message(Prepare):
    Node(1): rng → Abort
    outgoing: VoteAbort→Coordinator [enqueued t=6]
    Node(1): phase → Voted
  ✓ Agreement  ✓ Validity

Step 3 | t=3 | External(TickAll)
  Coordinator.tick() [Voting, 0 votes] → no-op (no spontaneous abort)
  Node(0).tick() → no-op
  Node(1).tick() → no-op
  ✓ Agreement  ✓ Validity

Step 4 | t=4 | Internal: Deliver Prepare → Node(0)
  implicit tick: Node(0) → no-op
  on_message(Prepare):
    Node(0): rng → Commit
    outgoing: VoteCommit→Coordinator [enqueued t=7]
    Node(0): phase → Voted
  ✓ Agreement  ✓ Validity

Step 5 | t=5 | External(TickAll)
  Coordinator.tick() [Voting, 0 votes] → no-op
  Node(0).tick() → no-op
  Node(1).tick() → no-op
  ✓ Agreement  ✓ Validity

Step 6 | t=6 | Internal: Deliver VoteAbort(Node(1)) → Coordinator
  implicit tick: Coordinator [Voting, 0 votes] → no-op
  on_message(VoteAbort from Node(1)):
    votes[Node(1)] = Abort
    try_decide() → abort vote exists → decide Abort
    Coordinator: phase → Decided, decision = Abort
  ✓ Agreement  ✓ Validity(Abort): no participant has committed ✓

Step 7 | t=7 | Internal: Deliver VoteCommit(Node(0)) → Coordinator
  implicit tick: Coordinator [Decided]
    → send DecAbort to all
    outgoing: DecAbort→Node(0) [enqueued t=10], DecAbort→Node(1) [enqueued t=12]
    phase → Done
  on_message(VoteCommit from Node(0)):
    phase == Done, guard rejects → ignored
  ✓ Agreement  ✓ Validity

Step 8 | t=9 | External(TickAll)
  Coordinator.tick() [Done] → no-op
  Node(0).tick() → no-op
  Node(1).tick() → no-op
  ✓ Agreement  ✓ Validity

Step 9 | t=10 | Internal: Deliver DecAbort → Node(0) [AND External(TickAll) at same t]
  implicit tick: Node(0) → no-op
  on_message(DecAbort):
    Node(0): phase → Decided(Abort)
  ✓ Agreement (only Node(0) decided: Abort)  ✓ Validity

Step 10 | t=10 | External(TickAll) [same timestamp, later sequence number]
  Coordinator.tick() [Done] → no-op
  Node(0).tick() → no-op
  Node(1).tick() → no-op
  ✓ Agreement  ✓ Validity

Step 11 | t=12 | Internal: Deliver DecAbort → Node(1)
  implicit tick: Node(1) → no-op
  on_message(DecAbort):
    Node(1): phase → Decided(Abort)
  ✓ Agreement (both decided Abort)  ✓ Validity

--- queue empty, simulation complete ---
Termination: ✓ (all participants decided)
```

Key observations:
- **Decision in on_message at Step 6**: the coordinator receives VoteAbort, records it, and
  `try_decide()` immediately decides Abort — no separate tick needed for the decision.
- **Implicit tick at Step 7** sends the decision messages: the coordinator was already in
  Decided (from Step 6), so the implicit tick before delivering VoteCommit triggers the
  broadcast. The VoteCommit itself is then ignored (phase is Done).
- **Message delivery order** differs from send order: Prepare→Node(1) arrives before
  Prepare→Node(0) due to jitter, even though both were sent in Step 1.
- **TickAll events at t=3, t=5** are harmless no-ops because no votes have arrived yet.
  The tail TickAll events are also no-ops. This is by design — the test over-provisions
  ticks to ensure the coordinator always gets a chance to act.

---

## Phase 1: Project Scaffold & Core Types

**Goal:** Establish the Cargo project, define message/event types, and define the
`StateMachine` trait.
**Checkpoint:** `cargo check` passes; types compile.

### Tasks
- [ ] Initialize Cargo project (`cargo init --lib`)
- [ ] Add dependencies:
  - `proptest` (with `features = ["attr-macro"]`)
  - `rand` + `rand_chacha` (for `ChaCha8Rng`)
- [ ] Define core types in `src/types.rs`:
  - `NodeId` — newtype over `u8`, derive `Copy`, `Ord`, `Hash`, `Debug`
  - `Phase` enum: `Waiting`, `Voting`, `Decided`, `Done`
  - `Decision` enum: `Commit`, `Abort`
  - `ActorId` enum: `Coordinator`, `Node(NodeId)`
  - `Message` struct: `message_type: MessageType`, `origin: ActorId`, `dest: ActorId`
    — derive `Clone`, `Debug`, `Eq`, `PartialEq`
  - `MessageType` enum: `StartTransaction`, `Prepare`, `VoteCommit`, `VoteAbort`,
    `DecCommit`, `DecAbort`
- [ ] Define `StateMachine` trait in `src/state_machine.rs`:
  ```rust
  pub trait StateMachine {
      fn on_message(&mut self, msg: &Message, at_time: u64) -> Vec<Message>;
      fn tick(&mut self, _at_time: u64) -> Vec<Message> { vec![] }
  }
  ```
- [ ] Set up `src/lib.rs` with module declarations

### Files to Create
- `Cargo.toml`
- `src/lib.rs`
- `src/types.rs`
- `src/state_machine.rs`

### Risks
- None significant. This is pure type definitions.

---

## Phase 2: Actor Implementations

**Goal:** Implement `Coordinator` and `Participant` as `StateMachine` impls.
**Checkpoint:** Minimal smoke tests pass for each actor in isolation.

### Tasks

#### Coordinator (`src/coordinator.rs`)
- [ ] Define `Coordinator` struct with fields per Actor Details above
- [ ] `Coordinator::new(nodes: Vec<NodeId>, rng_seed: u64, abort_bias: f64)`
- [ ] Implement `StateMachine::on_message`:
  - StartTransaction → send Prepare to all, phase → Voting
  - VoteCommit/VoteAbort → record in `votes` map, call `try_decide()`
- [ ] Implement `try_decide()`:
  - Any abort vote → decide Abort
  - All commit votes → decide Commit (prob 1-abort_bias) or Abort (prob abort_bias)
  - Otherwise → no decision
- [ ] Implement `StateMachine::tick`:
  - Voting → spontaneous Abort (prob abort_bias/10)
  - Decided → send DecCommit/DecAbort to all, phase → Done
- [ ] Add accessor methods: `phase()`, `decision()`, `votes()`, `nodes()`
- [ ] Smoke tests:
  - Receive StartTransaction → produces Prepare messages, transitions to Voting
  - Receive all commit votes → decides Commit
  - Receive any abort vote → decides Abort

#### Participant (`src/participant.rs`)
- [ ] Define `ParticipantPhase` enum: `Waiting`, `Voted`, `Decided(Decision)`
- [ ] Define `Participant` struct with fields per Actor Details above
- [ ] `Participant::new(id: NodeId, rng_seed: u64)`
- [ ] `Participant::with_fixed_vote(id: NodeId, vote: Decision)` — for edge-case tests
- [ ] Implement `StateMachine::on_message`:
  - Prepare (Waiting) → draw vote from RNG, send VoteCommit/VoteAbort, phase → Voted
  - DecCommit (Voted) → phase → Decided(Commit)
  - DecAbort (Voted) → phase → Decided(Abort)
- [ ] Add accessor methods: `phase()`, `vote()`, `decision()`, `has_voted()`
- [ ] Smoke tests:
  - Receive Prepare → sends vote message to coordinator
  - Receive DecCommit/DecAbort → records decision

### Files to Create
- `src/coordinator.rs`
- `src/participant.rs`

### Risks
- **Coordinator decide in on_message**: After recording a vote, `try_decide()` may
  transition to Decided. The next tick (implicit or explicit) will then broadcast the
  decision. If no tick follows, the coordinator stays in Decided indefinitely — the tick
  tail in tests prevents this.

---

## Phase 3: Simulation Harness

**Goal:** Build an event-queue simulator following `../reliable-channel`'s architecture.
**Checkpoint:** Simulator produces complete protocol traces that terminate.

### Tasks

#### Event Queue (`src/simulator/event.rs`)
- [ ] Define `ExternalEvent` enum: `StartTransaction`, `Tick { to: ActorId }`, `TickAll`
- [ ] Define `InternalEvent` enum: `Deliver { to: ActorId, msg: Message }`
- [ ] Define `Event` enum: `External(ExternalEvent)`, `Internal(InternalEvent)`
- [ ] Define `TimestampedEvent` struct: `timestamp: u64`, `sequence_number: u64`,
  `event: Event` — with reversed `Ord` for min-heap via `BinaryHeap`
- [ ] Define `EventQueue` struct: `BinaryHeap<TimestampedEvent>`,
  `next_sequence_number: u64`
  - `insert(timestamp, event)` — auto-assigns sequence number
  - `next() -> Option<(u64, Event)>` — pops earliest event

#### Simulator (`src/simulator.rs`)
- [ ] Define `Simulator` struct:
  - `coordinator: Coordinator`
  - `participants: BTreeMap<NodeId, Participant>`
  - `event_queue: EventQueue`
  - `clock: u64`
  - `rng: ChaCha8Rng`
  - `jitter_range: u64`
- [ ] `Simulator::new(n_participants, seed, abort_bias, jitter_range)`:
  - Derive per-actor RNG seeds from master seed
  - Create coordinator and participants
- [ ] `enqueue_external(event, at_time)` — insert external event into queue
- [ ] `enqueue_outgoing(messages)` — for each outgoing message, insert internal
  `Deliver` event with timestamp = `clock + 1 + rng.gen_range(0..jitter_range)`
- [ ] `deliver(to, msg) -> Vec<Message>`:
  - Call `actor.tick(clock)`, collect outgoing
  - Call `actor.on_message(&msg, clock)`, collect outgoing
  - Return all outgoing
- [ ] `step() -> bool`:
  - Pop next event from queue; if empty, return false
  - Advance clock to event timestamp
  - Match on event type:
    - `External(StartTransaction)` → deliver StartTransaction message to coordinator
    - `External(Tick { to })` → call `actor.tick(clock)`, enqueue outgoing
    - `External(TickAll)` → tick coordinator then each participant (BTreeMap order),
      enqueue all outgoing
    - `Internal(Deliver { to, msg })` → call `deliver(to, msg)`, enqueue outgoing
  - Check invariants
  - Return true
- [ ] `run()` — loop `step()` until it returns false

#### Properties (`src/properties.rs`)
- [ ] `check_agreement(participants) -> Result<(), String>`:
  - Among decided participants, all committed or all aborted
- [ ] `check_validity_commit(coordinator, participants) -> Result<(), String>`:
  - If coordinator decided Commit, then all votes in coordinator.votes() are Commit
    and votes.len() == nodes.len()
- [ ] `check_validity_abort(coordinator, participants) -> Result<(), String>`:
  - If coordinator decided Abort, no participant has committed
- [ ] `check_all_invariants(coordinator, participants) -> Result<(), String>`:
  - Calls all three checks; combines errors
- [ ] `is_terminated(participants) -> bool`:
  - All participants have decided

### Files to Create
- `src/simulator.rs` — module root, `Simulator` struct
- `src/simulator/event.rs` — `EventQueue`, event types
- `src/properties.rs` — invariant predicates

### Risks
- **Implicit tick in deliver**: When delivering a message, `tick()` fires before
  `on_message()`. If `tick()` transitions the coordinator (e.g., Decided → Done), the
  subsequent `on_message()` sees the new phase and guards reject. This is correct but
  requires all `on_message` handlers to gracefully ignore messages in unexpected phases.

---

## Phase 4: Property-Based Testing

**Goal:** Use proptest to run many simulations and verify protocol properties.
**Checkpoint:** `cargo test` passes; no property violations across many seeds.

### Tasks

#### proptest Strategies (`tests/simulation.rs`)
- [ ] Define `GenerateEvent` enum: `StartTransaction`, `TickAll`
- [ ] `event_strategy()` — weighted: StartTransaction (1), TickAll (5)
- [ ] `actions_strategy(max_len)` — `Vec<(GenerateEvent, u8)>` where `u8` is time delta
- [ ] `materialize_events(sim, actions)` — accumulate deltas into monotonic timestamps,
  enqueue as external events. Returns the final timestamp.
- [ ] `append_tick_tail(sim, after_time, count)` — append `count` TickAll events after
  the generated events, spaced 1 apart.

#### Property Tests
- [ ] `test_safety`:
  - Strategy: `n_participants` (1..=4), `seed` (u64), `abort_bias` (0..=200u32 mapped
    to 0.0..0.2), `jitter_range` (0..=10), `actions` (actions_strategy(30))
  - Setup simulator, materialize events, append tick tail (20)
  - Run step-by-step; invariants checked internally by simulator
  - `prop_assert!` succeeds if no panic
- [ ] `test_termination`:
  - Same strategy but with generous tick tail (50)
  - After simulation, `prop_assert!(is_terminated(participants))`

#### Deterministic Edge-Case Tests
- [ ] All-commit: both participants have fixed commit votes, abort_bias=0 →
  coordinator commits, both participants commit
- [ ] One-abort: one participant has fixed abort vote → coordinator aborts, both abort
- [ ] Coordinator abort despite all commits: abort_bias=1.0 → coordinator aborts
- [ ] Single participant: n=1, works correctly

### Files to Create
- `tests/simulation.rs`

### Risks
- **Shrinking quality**: proptest shrinks `u64` seeds by trying nearby values, which
  doesn't produce minimal counterexamples. If debugging is difficult, consider generating
  `Vec<usize>` (action indices) for structural shrinking.

---

## Dependencies Between Phases

```
Phase 1 ──→ Phase 2 ──→ Phase 3 ──→ Phase 4
```

Strictly sequential — each phase depends on the previous. Property predicates (Phase 3)
could be drafted alongside Phase 2 since they only inspect actor state.

## Risk Areas

1. **Implicit tick + on_message ordering** — The coordinator may decide in tick (spontaneous
   abort) before `on_message` processes the incoming vote. Guards must handle this.

2. **Tick tail sizing** — Too few tail ticks and the coordinator never broadcasts. Start
   with 20 and adjust based on test results.

3. **abort_bias tuning** — If abort_bias is too high, most traces abort (reducing coverage
   of the commit path). The proptest strategy generates abort_bias in 0.0–0.2.

## Design Decisions (from reliable-channel)

Patterns adopted from `../reliable-channel`:
- **Event queue** — min-heap of `TimestampedEvent` with sequence-number tiebreaker
- **External vs. internal events** — test orchestration (StartTransaction, Tick) vs.
  protocol messages (Deliver)
- **Implicit tick before delivery** — `tick()` fires before `on_message()` on every delivery
- **`ChaCha8Rng`** — portable deterministic RNG
- **`BTreeMap` for participants** — deterministic iteration order

Deliberate divergences:
- **Routing in `Message`** — our `Message` has typed `dest: ActorId` (matching Alloy's
  Message sig), vs. reliable-channel's `(String, Message)` tuples
- **No `Result` return** — no error paths in no-failure 2PC
- **No fault injection** — no-failure model; no drop/duplicate/bitflip. Future work.
- **Decision in on_message** — unlike reliable-channel where all logic is cleanly split
  between tick and on_message, the coordinator decides in on_message (via try_decide)
  when a vote triggers the condition. Spontaneous aborts still use tick.

## Open Items
- None.
