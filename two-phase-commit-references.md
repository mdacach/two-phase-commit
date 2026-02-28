# Two-Phase Commit: References & Reading Order

## Suggested Reading Order

Start with intuition, then formalize, then implement.

### Phase 1 — Build intuition (read/watch before writing any spec)

1. **Paper Trail: "Consensus Protocols: Two-Phase Commit"** (blog, ~15 min)
   https://www.the-paper-trail.org/post/2008-11-27-consensus-protocols-two-phase-commit/
   Clear explanation of the protocol, coordinator role, and blocking failure mode.
   Follow-ups on [3PC](https://www.the-paper-trail.org/post/2008-11-29-consensus-protocols-three-phase-commit/) and [Paxos](https://www.the-paper-trail.org/post/2009-02-03-consensus-protocols-paxos/) give the full arc of why 2PC isn't enough.

2. **DDIA Chapter 9: Consistency and Consensus** (book, ~45 min for the 2PC section)
   The practitioner-oriented treatment. Covers 2PC as atomic commit, the two
   "points of no return" (participant votes yes; coordinator decides), the
   blocking problem, and the relationship to consensus.

3. **Lamport's TLA+ Video Course — Lecture 6** (~21 min)
   https://lamport.azurewebsites.net/video/video6.html
   Script: https://lamport.azurewebsites.net/video/video6-script.pdf
   Walks through TCommit.tla and TwoPhase.tla step by step. Lamport explaining
   his own spec. Watch before reading the specs cold.

### Phase 2 — Formal specifications (TLA+ and Alloy)

4. **Gray & Lamport, "Consensus on Transaction Commit" (2006)** (paper, ~1 hr)
   https://lamport.azurewebsites.net/video/consensus-on-transaction-commit.pdf
   The key paper. Shows 2PC is a degenerate case of consensus (Paxos with F=0).
   Includes TLA+ specs for TCommit, TwoPhase, and PaxosCommit.
   Morning Paper summary: https://blog.acolyer.org/2016/01/13/consensus-on-transaction-commit/

5. **Lamport's TLA+ specs (tlaplus/Examples repo)**
   - TCommit.tla (abstract safety spec): https://github.com/tlaplus/Examples/blob/master/specifications/transaction_commit/TCommit.tla
   - TwoPhase.tla (the protocol): https://github.com/tlaplus/Examples/blob/master/specifications/transaction_commit/TwoPhase.tla
   - PaxosCommit.tla (stretch): https://github.com/tlaplus/Examples/blob/master/specifications/transaction_commit/PaxosCommit.tla
   Write your own spec first, then compare against these.

6. **Murat Demirbas, "TLA+/PlusCal modeling of 2PC" (2017)** (blog, ~30 min)
   Part 1: http://muratbuffalo.blogspot.com/2017/12/tlapluscal-modeling-of-2-phase-commit.html
   Part 2: http://muratbuffalo.blogspot.com/2017/12/tlapluscal-modeling-of-2-phase-commit_14.html
   PlusCal version exploring crash faults. Good companion while writing your spec.

7. **HASLab: Formal Software Design with Alloy 6 — Protocol Design**
   https://haslab.github.io/formal-software-design/protocol-design/index.html
   No published Alloy 2PC model exists — you'll build your own. This chapter
   gives the Alloy 6 patterns for distributed protocols (mutable relations,
   temporal properties, message passing). Your stop-and-wait Alloy experience
   transfers directly.

### Phase 3 — Rust + DST implementation

8. **UT Austin CS378: Rust Two-Phase Commit Lab**
   https://www.cs.utexas.edu/~rossbach/cs378-f23/lab/two-phase-commit-cs378.html
   Single-process 2PC simulator in Rust. Coordinator/participant as threads,
   "check mode" analyzes commit logs for correctness. Closest structural
   template to what you'd build for DST.

9. **P Language: Two Phase Commit Tutorial**
   http://p-org.github.io/P/tutorial/twophasecommit/
   P sits between TLA+ and Rust — generates executable code from specs. The
   "failure injector" state machine pattern is directly applicable to your
   DST fault model.

10. **S2.dev, "Deterministic simulation testing for async Rust"**
    https://s2.dev/blog/dst
    Practical walkthrough of making Rust DST actually deterministic. Addresses
    real gotchas (getrandom, clock_gettime). Useful for extending your existing
    DST framework.

### Phase 4 — Deepen understanding (optional / stretch)

11. **Daniel Abadi, "It's Time to Move on from Two Phase Commit" (2019)**
    http://dbmsmusings.blogspot.com/2019/01/its-time-to-move-on-from-two-phase.html
    Why 2PC is problematic in practice (blocking, latency, contention) and
    alternatives (deterministic databases, Percolator-style). Important
    counterpoint after you understand the protocol deeply.

12. **CockroachDB: Parallel Commits**
    https://www.cockroachlabs.com/blog/parallel-commits/
    How CockroachDB optimized 2PC with a STAGING state, cutting commit latency
    in half. The design was verified with TLA+ (Hillel Wayne ran an internal
    workshop). Real-world formal methods success story.

13. **Lamport's TLA+ Video Course — Lecture 7** (Paxos Commit)
    https://lamport.azurewebsites.net/video/video7-script.pdf
    Extends 2PC to tolerate coordinator failure using Paxos. Stretch goal:
    verify the refinement chain TCommit <- TwoPhase <- PaxosCommit.

14. **Marc Brooker, "Exploring TLA+ with two-phase commit" (2013)**
    https://brooker.co.za/blog/2013/01/20/two-phase.html
    AWS distinguished engineer's practitioner perspective on getting value
    from TLA+ model checking, using 2PC as the example.

---

## Quick Reference: Key Properties to Verify

From Gray/Lamport and DDIA:

| Property | Type | Statement |
|----------|------|-----------|
| Atomic commit | Safety | All participants that decide reach the same decision |
| Validity | Safety | If all vote yes and no failures, the decision is commit |
| No unilateral commit | Safety | No participant commits until all have voted yes |
| Abort on no-vote | Safety | If any participant votes no, the decision is abort |
| Blocking | Liveness (violated) | If the coordinator crashes after prepare, participants may wait forever |

---

## DST Frameworks (Rust)

For reference when implementing:

- **turmoil** (Tokio): https://github.com/tokio-rs/turmoil — deterministic network simulation
- **madsim**: https://github.com/madsim-rs/madsim — deterministic async runtime (used by RisingWave)
- **Your existing framework** from stop-and-wait — may be sufficient; evaluate whether to extend or adopt turmoil/madsim
