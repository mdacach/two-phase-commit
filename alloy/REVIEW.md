# TwoPhaseCommit.als Review Tracker

## Review Items

### R1: Rename MType → MessageType (line 9)
**Status:** Accepted
**Opinion:** Agree — no reason to abbreviate, and `MessageType` is clearer.

### R2: Rename Msg → Message (line 12)
**Status:** Accepted
**Opinion:** Agree — same reasoning. The field `messageType: one MessageType` reads naturally on `sig Message`.

### R3: Rename mtype → messageType (line 14)
**Status:** Accepted
**Opinion:** Agree — follows Alloy's camelCase convention for fields.

### R4: Introduce Actor supertype for message destination (lines 19-22)
**Status:** Accepted
**Opinion:** Agree — this is the biggest structural improvement. Currently vote messages
use `dest = n` to mean "vote FROM participant n", which is misleading since the message
is actually going TO the coordinator. With `abstract sig Actor`, `one sig Coordinator
extends Actor`, and `sig Node extends Actor`, messages get proper `origin: one Actor` and
`dest: one Actor` fields. Vote messages become `origin = n, dest = Coordinator`, matching
their real semantics.

### R5: Explain why messages are never removed from Sent (line 24)
**Status:** Accepted
**Opinion:** Agree. The ever-growing Sent set is a standard simplification (from Lamport's
TLA+ specifications). Messages are never consumed on reception — they stay available
forever. This means: (1) broadcast messages (Prepare, Decision) are naturally available to
all recipients without duplication, (2) no delivery ordering is needed, (3) the state space
is smaller. The trade-off is that the model can't express message loss (which is fine for
the no-failure model).

### R6: Coordinator status as field + enum (lines 30-31)
**Status:** Accepted
**Opinion:** Agree — the coordinator is a single entity, so `one sig Coordinator` with
`var phase: one Phase` is cleaner than 4 separate `var sig` subsets of Node (which abused
Node membership as a boolean). The per-participant state (HasVoted, ParticipantDecided,
etc.) correctly remains as `var sig` subsets since that's genuinely per-element state.

### R7: Document monotonicity fact, better name (line 73)
**Status:** Accepted
**Opinion:** Rename to `messagesNeverLost`. Add a comment explaining this encodes
reliable delivery (no message loss) as a fact of the model.

### R8: Explain abort decision in coordinatorReceiveVote (line 166)
**Status:** Accepted (subsumed by R9)
**Opinion:** The abort-on-receive behavior gets replaced by a separate predicate (R9),
which makes the explanation part of the predicate's own documentation.

### R9: Separate "decides abort" and "decides commit" (line 180)
**Status:** Accepted
**Opinion:** Agree — clearer intent. `coordinatorReceiveVote` becomes pure vote
recording. Two new predicates: `coordinatorDecideCommit` (guard: all voted commit) and
`coordinatorDecideAbort` (guard: some received vote is abort). This also means the
coordinator no longer immediately aborts upon receiving an abort vote — it can process
more votes first, then decide. The properties still hold; the separation adds
interleavings but no new behaviors.

### R10: Fairness table with explanations (line 291)
**Status:** Accepted
**Opinion:** Agree — fairness assumptions are the hardest part to review. A table with
"condition → guarantee → why" for each clause makes the assumptions auditable.

### R11: Scope calculation for messages and steps (line 365)
**Status:** Accepted
**Opinion:** Agree. Messages = 5 × N (one per type per node). Minimum steps for a
complete trace = 3N + 3 (sendPrepare + N votes + N receiveVotes + decide + sendDecision +
N receiveDecisions). We use generous padding for stutter and nondeterminism.

### R12: Why separate check commands per scope? (line 372)
**Status:** Fixed — consolidated
**Opinion:** We don't need separate `Agreement_3`, `Agreement_4` etc. Alloy allows
multiple `check Agreement for ...` commands with different scopes. The earlier approach
was a workaround for a syntax issue (`check Name : Assertion for scope` doesn't parse),
but simply repeating `check Agreement for exactly 3 Node, ...` works fine.

### R13: Replace HasVoted/ParticipantDecided with symmetric pairs (line 38-40)
**Status:** Accepted
**Opinion:** Agree — `VotedCommit`/`VotedAbort` is more symmetric and self-documenting
than `HasVoted`/`VotedCommit`. Same for `ParticipantCommitted`/`ParticipantAborted` vs
`ParticipantDecided`/`ParticipantCommitted`. The "has voted" / "has decided" booleans
become derivable: `HasVoted = VotedCommit + VotedAbort`, `ParticipantDecided =
ParticipantCommitted + ParticipantAborted`. We can express these as `fun` helpers if
needed, but the primary state uses the concrete outcome sets.

### R14: Move frame condition helpers after event predicates (line 66)
**Status:** Accepted
**Opinion:** Agree — readers see the events first (the interesting part), then the
helpers (mechanical plumbing). The helpers are only meaningful in context of the events
that use them, so placing them after is a better reading order.

### R15: Coordinator free to abort without an abort vote (line 157-158)
**Status:** Accepted
**Opinion:** Agree — in real 2PC the coordinator can unilaterally abort (e.g., timeout,
internal policy). Removing the abort-vote guard from `coordinatorDecideAbort` makes the
model more general. The only guard is `Coordinator.phase = Voting`. The Validity
assertion still holds because it only constrains the commit path: "commit implies all
voted commit." The abort path has no such constraint — the coordinator can abort for
any reason. This also simplifies the fairness clause: the "some abort vote" condition
becomes just "coordinator is voting and hasn't decided yet," but we need to be careful
not to force abort under fairness when commit is also possible. We'll adjust fairness
clause 5 to only fire when `coordinatorDecideCommit` is not enabled, preventing
spurious aborts when all votes are actually commit.

### R16: Relax fairness — coordinator should decide, not be forced to commit (line 275-277)
**Status:** Accepted
**Opinion:** Agree. The current fairness has two separate clauses (4 and 5) that
independently force commit and abort. Clause 4 forces commit when all votes are commit,
which is too strong — the coordinator should be free to abort even then. The fix: merge
clauses 4 and 5 into a single clause that says "if coordinator is voting, it eventually
decides (commit or abort)." This is weaker and more realistic — the coordinator must
make _a_ decision, but the choice is unconstrained. The Validity assertion still
prevents incorrect commits (commit only if all voted commit), and the unrestricted
abort (R15) allows abort at any time. Termination still holds because the coordinator
eventually decides _something_.

Experiment plan: try removing clauses 4 and 5 and replacing with a single
"phase = Voting implies eventually (coordinatorDecideCommit or coordinatorDecideAbort)".
Check if Termination still passes. If so, try removing other clauses to find the
minimal fairness set.

### R17: Remove "E" prefix from Event enum variants (line 318)
**Status:** Accepted
**Opinion:** Agree — the prefix was there to avoid name clashes with predicates, but
enum variants and predicates live in different namespaces in Alloy. `CoordSendPrepare`
etc. are clear without the prefix. However, I need to verify there's no actual name
clash with the pred names (which use lowercase `coordinatorSendPrepare`). Since Alloy
is case-sensitive and enum variants are PascalCase, there's no conflict.

---

## Fairness Relaxation Experiment (R16)

Tested three fairness configurations against Termination at scope 2:

| Configuration | Clauses kept | Termination |
|--------------|--------------|-------------|
| Full (merged 4+5) | 1,2,3,4,5,6 | Pass |
| No receive (drop 3) | 1,2,4,5,6 | Pass |
| Minimal (drop 2+3) | 1,4,5,6 | Pass |

**Finding:** Clauses 2 (participants vote) and 3 (coordinator receives votes) are not
needed for Termination. The coordinator can always abort without receiving any votes,
so only clauses 1 (start), 4 (decide), 5 (send decision), and 6 (receive decision) are
needed.

**Decision:** Keep clauses 2 and 3 but weaken clause 3 to use WF-style encoding
(`eventually always P implies eventually Q`). This allows the coordinator to decide
without processing all available votes, which is more realistic and makes the
specification stronger (more traces explored). Clauses 2 and 3 are not needed for
Termination but ensure the model can explore the commit path.
