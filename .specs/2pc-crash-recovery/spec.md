# Specification: 2pc-crash-recovery

**Status:** Implemented
**Created:** 2026-03-10
**Last Updated:** 2026-03-11

## Context

The Rust 2PC implementation (`.specs/2pc-rust`) covers the no-failure model.
This spec extends it with crash recovery: node crashes, durable state (WAL),
retransmission, and acknowledgement.

## Problem

Without crash recovery, the protocol halts permanently if any actor crashes
after voting. The coordinator cannot distinguish a crashed participant from a
slow one, and participants cannot learn the decision if the coordinator dies.

## Goals

- [x] Add retransmission and acknowledgement to the protocol.
- [x] Model node crashes and recovery in the simulator.
- [x] Add write-ahead logs (WAL) for durable state across crashes.
- [x] Verify Agreement, Validity, and Termination under crash scenarios via
  property-based testing.

## Non-Goals

- No network partitions independent of crashes.
- No Byzantine faults.
- No message loss without a crash (crashes subsume message loss).

## Design Decisions

- **Crashes modeled in the simulator layer.** The simulator tracks which actors
  are alive and drops messages to dead actors. Actors have no `alive` flag.
- **Durable state (WAL) owned by the actor.** Each actor holds a `Wal` struct
  representing on-disk state that survives crashes. The simulator never touches
  it directly.
- **Recovery is `actor.recover()`.** Reads from the WAL and resets volatile
  state. Called by the simulator on a `Recover` event. Conceptually equivalent
  to the actor restarting and reading its write-ahead log from disk.
- **Retransmission is timeout-gated.** Actors only retransmit on `tick` if at
  least `retransmit_timeout` time has elapsed since the last send. The timeout
  is configurable per-actor.
- **No separate message loss model.** Node crashes subsume message loss — a
  crash drops all in-flight messages to/from the node.

## Technical Approach

### Phase 1: Retransmission & Acknowledgement

No crashes yet. Add the protocol machinery that crash recovery will depend on.

#### Coordinator

- Add `AwaitingAcks` phase between `Decided` and `Done`.
- Track acks in a `BTreeSet<NodeId>`; transition to `Done` when all acked.
- Record `last_prepare_time: Option<u64>` and `last_decision_time: Option<u64>`.
- `tick` in `Voting`: if `clock - last_prepare_time >= retransmit_timeout`,
  retransmit Prepare to nodes with no recorded vote. Update timestamp.
- `tick` in `AwaitingAcks`: if `clock - last_decision_time >= retransmit_timeout`,
  retransmit Decision to unacked nodes. Update timestamp.

#### Participant

- Send `Ack` message on receiving Decision.
- Handle duplicate Prepare while in `Voted`: re-send vote (idempotent).
- Handle duplicate Decision while in `Decided`: re-send Ack (idempotent).

#### Types

- Add `MessageType::Ack`.

#### Property tests

Run existing `test_safety` and `test_termination` — must still pass.
Retransmission with no crashes should not change observable behavior, only add
redundant messages.

### Phase 2: Crash/Recover in Simulator

#### Simulator

- `alive: BTreeMap<ActorId, bool>` tracks liveness of every actor.
- New `ExternalEvent` variants: `Crash(ActorId)`, `Recover(ActorId)`.
- `Deliver` to a dead actor: drop the message, log as `LogEntry::Drop`.
- `tick`/`TickAll`: skip dead actors.
- `Recover` event: call `actor.recover()`, mark alive.

#### LogEntry

- Add `Drop { at, msg }` variant for visibility into messages lost to crashes.

#### Property tests

- proptest generates `Crash`/`Recover` events intermixed with ticks.
- **Safety tests**: allow permanent crashes. Check agreement/validity only among
  decided nodes.
- **Termination tests**: add a recovery sweep at the end (recover all nodes,
  tick until quiescent). All participants must decide.

### Phase 3: Durable State (WAL)

#### Participant

- Add `Wal { vote: Option<Decision> }` field.
- Write vote to WAL *before* sending VoteCommit/VoteAbort.
- `recover()`: if `wal.vote` is `Some(d)` → reset to `Voted(d)`;
  if `None` → reset to `Waiting`.

#### Coordinator

- Add `Wal { decision: Option<Decision> }` field.
- Write decision to WAL *before* sending DecisionCommit/DecisionAbort.
- `recover()`: if `wal.decision` is `Some(d)` → reset to `Decided(d)`, clear
  acks, retransmit on next tick; if `None` → reset to `Voting`, clear votes,
  retransmit Prepare on next tick.

#### Property tests

Full proptest with independent crashes of any actor at any time, including
simultaneous crashes of coordinator and participants.

- **Agreement** (AC1): all decided participants agree. Must hold.
- **Validity** (AC2): commit requires unanimous commit votes. Must hold.
- **Termination**: given eventual recovery of all nodes, all participants
  must eventually decide.

## Edge Cases

| Case | Expected Behavior |
|------|-------------------|
| Coordinator crashes before deciding | Recovers into Voting, retransmits Prepare, re-collects votes |
| Coordinator crashes after deciding | Recovers into Decided (from WAL), retransmits Decision |
| Participant crashes after voting | Recovers into Voted (from WAL), re-sends vote on duplicate Prepare |
| Participant crashes after deciding | Recovers into Decided (from WAL), re-sends Ack on duplicate Decision |
| All actors crash simultaneously | After recovery, coordinator retransmits; protocol completes |
| Coordinator crashes, participant receives no decision | Participant blocks in Voted until coordinator recovers |

## Acceptance Criteria

- [x] Retransmission and Ack work without crashes (existing tests pass)
- [x] Crash/Recover events in simulator drop messages and call `recover()`
- [x] WAL persists decision (coordinator) and vote+decision (participant)
- [x] Agreement and Validity hold at every step under crash scenarios
- [x] Termination holds given eventual recovery of all actors
- [x] proptest finds no violations across many random seeds with crashes

### How to Verify

`cargo test` — all unit tests and proptest property tests pass, including
`test_safety_with_crashes` and `test_termination_with_crashes`.

## Agent Rules

- Do not change scope beyond Goals/Non-goals.
- If something is ambiguous, add it to **Open Questions** and stop.

## Open Questions

None.

## References

- `.specs/2pc-rust/spec.md` — no-failure baseline spec
- `src/coordinator/mod.rs`, `src/participant/mod.rs` — authoritative rustdoc
- `.specs/2pc-rust/reviews.md` — design decisions from code review
