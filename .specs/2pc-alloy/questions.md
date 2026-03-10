# Q&A Log: 2pc-alloy

Iteration history from `/iterate-spec` sessions.

---

## Iteration 1 - 2026-03-04

### Questions Asked

**Clarifications**

1. The spec says "two properties" but lists three (A, B, C). The TLA+ spec checks Agreement (safety), Consistency (safety), and Termination (liveness). Should the Alloy spec check all three, matching the TLA+ spec?

   > **Answer:** Yes. It's three, sorry xD.

2. Property A says "all nodes agree on the decision" and B says "nodes' decision match coordinator's." In the TLA+ spec, Agreement checks that decided participants agree with each other, and Consistency checks that a COMMIT decision implies all votes were VOTE_COMMIT. Should the Alloy properties mirror the TLA+ formulations exactly?

   > **Answer:** Do not mirror the TLA+ formulations exactly.

3. What does "done" mean for properties A and B? In the TLA+ spec, Agreement holds as an invariant at every state (any two decided participants agree), not just at the end. Should Alloy do the same (check as an `always` invariant)?

   > **Answer:** Yes, if it's possible to model as an invariant do that. The specification must cover some "done" state (not necessarily explicitly, though).

**Alloy-Specific Concerns**

4. The TLA+ spec uses weak fairness (`WF_vars(Next)`) to ensure Termination. Alloy 6 doesn't have built-in fairness constraints — should we encode fairness explicitly (e.g., `always eventually` guards on each action), or is checking termination under fairness out of scope?

   > **Answer:** Fairness is important. You can model fairness explicitly with Alloy's temporal operators.

5. Should the Alloy model closely mirror the TLA+ structure (coordinator phases, participant phases, votes as mutable state) or take a more idiomatic Alloy approach (e.g., using `var sig` subsetting for phases)?

   > **Answer:** Idiomatic Alloy approach. Take TLA+'s spec only as the slightest inspiration, and only if needed.

6. What scope should we target for checking? (e.g., exactly 3 participants, or parameterized with a range?)

   > **Answer:** Check for multiple sensible scopes.

**Edge Cases**

7. In the TLA+ spec, a participant can receive a decision even if it hasn't voted yet (if another participant voted ABORT). Should the Alloy model preserve this behavior?

   > **Answer:** Yes, it's allowed by the two-phase protocol.

8. The spec says no failure modes (crashes, timeouts). Just confirming: the model assumes all nodes stay up and all messages eventually get delivered — correct?

   > **Answer:** Yes. Create a first version where no failure happens, and then research and create a report on needed changes to cover failures too.

### Spec Changes Made
- Fixed "two properties" → "three properties" with clear labels (Agreement, Validity, Termination)
- Specified Agreement and Validity as `always` invariants
- Added explicit fairness requirement using temporal operators
- Set style to idiomatic Alloy 6 (not a TLA+ port)
- Added edge case: participant receives decision before voting
- Added scope: check multiple scopes (2, 3, 4 participants)
- Added goal: research report on failure mode extensions
- Added no-failure assumption to Technical Approach
- Updated status to Ready
- Cleared Open Questions
