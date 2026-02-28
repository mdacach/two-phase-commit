# Q&A Log: 2PC No Failures

Iteration history from `/iterate-spec` sessions.

---

## Iteration 1 - 2026-02-28

### Questions Asked

**Clarifications**

1. Should the number of participants be fixed at exactly 2, or should it be a parameter (e.g., a CONSTANT `Participants`) so TLC can check with different counts?

   > **Answer:** The number of participants should be parameterized.

2. Is each participant's vote nondeterministic (freely chooses VOTE_COMMIT or VOTE_ABORT), or is there a predicate that determines how it votes?

   > **Answer:** Each participant freely chooses their own vote.

3. The spec mentions a coordinator "gathering votes" — does the coordinator wait until it has received a vote from every participant before deciding, or can it abort early upon seeing a single VOTE_ABORT?

   > **Answer:** It can abort early upon seeing a VOTE_ABORT.

**TLA+ Modeling**

4. Should the network be modeled as a set of messages (unordered bag, where a message is a record like `[type |-> "PREPARE", src |-> id, dst |-> id]`), or as per-participant queues/channels?

   > **Answer:** Unordered message set. It's the standard TLA+ idiom, produces a smaller state space, and extends naturally to failure modeling later.

5. Should we write this in PlusCal (transpiled to TLA+) or in raw TLA+?

   > **Answer:** Write this in raw TLA+.

**Properties**

6. For the safety property "all participants agreed on the same value" — should we also verify the stronger consistency property: if any participant voted ABORT, then the final decision must be ABORT (and COMMIT only if all voted COMMIT)?

   > **Answer:** Let's verify the stronger consistency property too.

7. Should we check any liveness properties (e.g., the protocol eventually terminates with a decision), or only safety/invariants for this spec?

   > **Answer:** Check that the protocol eventually terminates with a decision too.

**Acceptance Criteria**

8. Given the above, here are proposed acceptance criteria — are these sufficient, or would you add/change any?
   - TLC checks the spec without errors for the configured number of participants
   - Safety: all participants that have decided hold the same decision value
   - Consistency: COMMIT decision only occurs when all votes were VOTE_COMMIT

   > **Answer:** Accepted.

### Spec Changes Made
- Updated Technical Approach with all decisions (parameterized participants, nondeterministic voting, early abort, unordered message set, raw TLA+)
- Filled in Edge Cases table
- Added safety, consistency, and liveness properties
- Filled in Acceptance Criteria
- Cleared Open Questions
- Updated status to Ready
