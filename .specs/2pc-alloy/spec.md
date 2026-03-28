# Specification: 2pc-alloy

**Status:** Implemented
**Created:** 2026-03-04
**Last Updated:** 2026-03-11

## Context
A TLA+ specification for two-phase commit already exists, and now we want to create a
specification of the same protocol using Alloy 6. The Alloy spec should be idiomatic Alloy,
not a port of the TLA+ spec.

## Problem
N/A.

## Goals
- [x] Create an Alloy 6 specification for two-phase commit (no-failure model).
- [x] Research and report on changes needed to cover failure modes.

## Non-Goals
N/A.

## Interface
N/A.

### Example Usage
N/A.

## Technical Approach

The specification will cover exactly one transaction. Use idiomatic Alloy 6 style
(temporal operators, `var` fields, `var sig` subsetting, `event` predicates, etc.).

**Coordinator** handles the two-phase protocol:
- Sends "prepare" messages to all participants.
- Receives votes ("vote_commit", "vote_abort").
- Decides commit/abort. A commit decision requires "vote_commit" from all participants.
- Sends decision ("decision_commit", "decision_abort").

**Network** is modeled as an ever-increasing set of messages (messages are never removed,
modeling duplicate delivery).

**Participants** spontaneously choose their vote upon receiving a prepare message.
A participant may receive a decision before voting (e.g., if another participant voted abort).

**No failure model** in the first version — all nodes stay up and all messages are
eventually delivered.

### Properties

Three properties to check:

A. **Agreement** (safety, `always` invariant): All decided participants agree on the same
   decision at every reachable state.

B. **Validity** (safety, `always` invariant): A commit decision implies all participants
   voted to commit.

C. **Termination** (liveness): Eventually all participants reach a decision.

Properties should be expressed idiomatically in Alloy, not as direct translations from TLA+.

### Fairness

Fairness must be modeled explicitly using Alloy's temporal operators (e.g., `always eventually`
guards on enabled actions) to prevent trivial counterexamples to Termination.

### Event Reification

Add reified events (enum + functions mapping events to actors) so that events are visible
in the Alloy GUI visualizer. Follow the pattern used in `alloy/scratch.als`.

### Scope

Check properties for multiple sensible scopes (e.g., 2, 3, 4 participants).

## Constraints
- Performance: none
- Compatibility: Alloy 6
- Dependencies: none

## Edge Cases
| Case | Expected Behavior |
|------|-------------------|
| Participant receives decision before voting (another voted abort) | Participant accepts the decision directly |
| All participants vote commit | Coordinator decides commit, sends commit to all |
| Any participant votes abort | Coordinator decides abort, sends abort to all |

## Acceptance Criteria
- [x] Reasoned through examples with /alloy-cli.
- [x] All safety and liveness properties pass across multiple scopes.
- [x] Specification follows Alloy 6 best practices (idiomatic style).
- [x] Event reification works in the visualizer.
- [x] Research report on failure mode extensions delivered.

### How to Verify
The /alloy-cli skill allows you to run alloy commands on the command line.
Verifying the work consists of running that skill with varied inputs and reasoning through
the outputs. At the end of the implementation, the properties should also be checked by
running that skill.

## Agent Rules
- Do not change scope beyond Goals/Non-goals.
- If something is ambiguous, add it to **Open Questions** and stop.
- Run verification commands after each phase.

## Open Questions
- None remaining.

## References
- `tla/TwoPhaseCommit.tla` — existing TLA+ spec (for reference, not to be ported)
- `alloy/scratch.als` — event reification pattern example
