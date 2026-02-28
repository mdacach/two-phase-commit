# Specification: 2PC No Failures

**Status:** Implemented
**Created:** 2026-02-28
**Last Updated:** 2026-02-28

## Context
This will be the first specification in this project, nothing exists yet.

## Problem
N/A.

## Goals
- [ ] Create a TLA+ specification for two-phase commit, without accounting for failures.

## Non-Goals
- Not writing any code.
- Not adding failure handling to the specification.

## Interface
N/A.

## Technical Approach

### Participants
- One coordinator and a parameterized set of participants (CONSTANT `Participants`).
- Participants are identified by their elements in the `Participants` set.

### Network
- An unordered set of message records stored in a single `messages` variable.
- Each message is a record with at least `type`, `source`, and `destination` fields.
- Sending = adding a record to the set. Receiving = matching and removing (or reading) from the set.

### Message Types
- `PREPARE`: coordinator -> participant, requesting a vote.
- `VOTE_COMMIT` / `VOTE_ABORT`: participant -> coordinator, the participant's freely chosen vote (nondeterministic).
- `COMMIT` / `ABORT`: coordinator -> participant, the final decision.

### State
- `votes`: mapping participant -> vote, tracked by the coordinator as votes arrive.
- `decision`: mapping participant -> decision, each participant's persisted decision.
- Coordinator and participant phase/status as needed to guard actions.

### Protocol
**Phase 1 — Voting:**
1. Coordinator sends `PREPARE` to all participants.
2. Each participant nondeterministically chooses `VOTE_COMMIT` or `VOTE_ABORT` and sends it to the coordinator.

**Phase 2 — Decision:**
3. Coordinator reads votes from the network. It may abort early upon receiving any `VOTE_ABORT`.
4. If all votes are `VOTE_COMMIT`, coordinator decides `COMMIT`; otherwise `ABORT`.
5. Coordinator sends the decision to all participants.
6. Each participant receives the decision and persists it.

### Language
Raw TLA+ (no PlusCal).

### Properties to Verify

**Safety (invariants):**
- **Agreement:** All participants that have decided hold the same decision value.
- **Consistency:** A `COMMIT` decision occurs only if every participant voted `VOTE_COMMIT`.

**Liveness (temporal):**
- **Termination:** The protocol eventually terminates with every participant holding a decision.

## Edge Cases
| Case | Expected Behavior |
|------|-------------------|
| All participants vote COMMIT | Coordinator decides COMMIT; all participants persist COMMIT |
| All participants vote ABORT | Coordinator decides ABORT; all participants persist ABORT |
| Mixed votes (some COMMIT, some ABORT) | Coordinator decides ABORT; all participants persist ABORT |
| Single participant in the system | Protocol still completes correctly |

## Acceptance Criteria
- [ ] TLC checks the spec without errors for the configured number of participants
- [ ] Safety: all participants that have decided hold the same decision value
- [ ] Consistency: COMMIT decision only occurs when all votes were VOTE_COMMIT
- [ ] Liveness: the protocol eventually terminates with all participants decided

### How to Verify
Verification should be done by running tests on the specification with the /tlc-cli skill.

## Agent Rules
- Do not change scope beyond Goals/Non-goals.
- If something is ambiguous, add it to **Open Questions** and stop.
- Run verification commands after each phase.

## Open Questions
None.

## References
None.
