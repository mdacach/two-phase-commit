module TwoPhaseCommit

-- ======== Actors ========

abstract sig Actor {}
sig Node extends Actor {}

enum Phase { Waiting, Voting, Decided, Done }
enum Decision { Commit, Abort }

one sig Coordinator extends Actor {
    var phase: one Phase,
    var decision: lone Decision,
    var votesReceived: set Node,
    var commitVotes: set Node
}

-- ======== Messages ========

enum MessageType { Prepare, VoteCommit, VoteAbort, DecCommit, DecAbort }

sig Message {
    messageType: one MessageType,
    origin: one Actor,
    dest: one Actor
}

-- Ever-growing network: once sent, a message stays in Sent forever. Messages are
-- never consumed on reception — they remain available indefinitely. This simplifies
-- the model: (1) broadcast messages (Prepare, Decision) are naturally available to
-- all recipients without duplication, (2) no delivery ordering is needed. The
-- trade-off is that the model cannot express message loss, which is acceptable for
-- the no-failure variant of 2PC.
var sig Sent in Message {}

-- ======== Participant State ========

var sig VotedCommit in Node {}
var sig VotedAbort in Node {}
var sig ParticipantCommitted in Node {}
var sig ParticipantAborted in Node {}

-- Derived sets for convenience.
fun hasVoted : set Node { VotedCommit + VotedAbort }
fun participantDecided : set Node { ParticipantCommitted + ParticipantAborted }

-- ======== Message Filter Helpers ========

-- These reduce duplication between event guards and fairness clauses.
fun prepareFor[n: Node] : set Message {
    { m: Sent | m.messageType = Prepare and m.dest = n }
}
fun voteFrom[n: Node] : set Message {
    { m: Sent | m.messageType in (VoteCommit + VoteAbort) and m.origin = n and m.dest = Coordinator }
}
fun decisionFor[n: Node] : set Message {
    { m: Sent | m.messageType in (DecCommit + DecAbort) and m.dest = n and m.origin = Coordinator }
}

-- ======== Initial State ========

fact init {
    no Sent
    Coordinator.phase = Waiting
    no Coordinator.decision
    no Coordinator.votesReceived
    no Coordinator.commitVotes
    no VotedCommit
    no VotedAbort
    no ParticipantCommitted
    no ParticipantAborted
}

-- ======== Network Reliability ========

-- Messages are never lost: anything in Sent stays in Sent forever. This encodes
-- reliable delivery as a structural property of the model. To model message loss,
-- this fact would be relaxed.
fact messagesNeverLost {
    always Sent in Sent'
}

-- ======== Event Predicates ========

-- 1. Coordinator broadcasts Prepare to all participants.
pred coordinatorSendPrepare {
    -- Guard
    Coordinator.phase = Waiting

    -- Effect
    Sent' = Sent + { m: Message | m.messageType = Prepare and m.origin = Coordinator }
    Coordinator.phase' = Voting
    unchangedCoordDecision
    unchangedCoordVotes
    unchangedParticipant
}

-- 2. Participant n votes (free to vote commit or abort).
pred participantVote[n: Node] {
    -- Guard
    n not in hasVoted
    some prepareFor[n]

    -- Effect: send a vote message to the coordinator; mark n as voted.
    (
        (Sent' = Sent + { m: Message | m.messageType = VoteCommit and m.origin = n and m.dest = Coordinator }
         and VotedCommit' = VotedCommit + n
         and VotedAbort' = VotedAbort)
        or
        (Sent' = Sent + { m: Message | m.messageType = VoteAbort and m.origin = n and m.dest = Coordinator }
         and VotedAbort' = VotedAbort + n
         and VotedCommit' = VotedCommit)
    )

    unchangedParticipantDecision
    unchangedCoordinator
}

-- 3. Coordinator records a vote from participant n.
pred coordinatorReceiveVote[n: Node] {
    -- Guard
    Coordinator.phase = Voting
    n not in Coordinator.votesReceived
    let vote = voteFrom[n] | some vote

    -- Effect
    Coordinator.votesReceived' = Coordinator.votesReceived + n
    (some m: Sent | m.messageType = VoteCommit and m.origin = n and m.dest = Coordinator)
        implies Coordinator.commitVotes' = Coordinator.commitVotes + n
        else    Coordinator.commitVotes' = Coordinator.commitVotes

    unchangedCoordPhase
    unchangedCoordDecision
    unchangedSent
    unchangedParticipant
}

-- 4. Coordinator decides. The coordinator is free to abort at any point during the
-- voting phase — it does not need to have received an abort vote. Commit requires
-- that all participants voted commit.
pred coordinatorDecide[d: Decision] {
    -- Guard
    Coordinator.phase = Voting
    d = Commit implies Coordinator.commitVotes = Node

    -- Effect
    Coordinator.phase' = Decided
    Coordinator.decision' = d
    unchangedCoordVotes
    unchangedSent
    unchangedParticipant
}

-- 5. Coordinator broadcasts its decision to all participants.
pred coordinatorSendDecision {
    -- Guard
    Coordinator.phase = Decided

    -- Effect
    Coordinator.decision = Commit
        implies Sent' = Sent + { m: Message | m.messageType = DecCommit and m.origin = Coordinator }
        else    Sent' = Sent + { m: Message | m.messageType = DecAbort  and m.origin = Coordinator }

    Coordinator.phase' = Done
    unchangedCoordDecision
    unchangedCoordVotes
    unchangedParticipant
}

-- 6. Participant n receives the coordinator's decision.
pred participantReceiveDecision[n: Node] {
    -- Guard
    n not in participantDecided
    some decisionFor[n]

    -- Effect
    (some m: Sent | m.messageType = DecCommit and m.dest = n)
        implies (ParticipantCommitted' = ParticipantCommitted + n
                 and ParticipantAborted' = ParticipantAborted)
        else    (ParticipantAborted' = ParticipantAborted + n
                 and ParticipantCommitted' = ParticipantCommitted)

    unchangedVotes
    unchangedSent
    unchangedCoordinator
}

-- 7. Stutter: nothing changes.
pred stutter {
    unchangedSent
    unchangedCoordinator
    unchangedParticipant
}

-- ======== Frame Condition Helpers ========

pred unchangedSent { Sent' = Sent }

pred unchangedCoordPhase { Coordinator.phase' = Coordinator.phase }
pred unchangedCoordDecision { Coordinator.decision' = Coordinator.decision }
pred unchangedCoordVotes {
    Coordinator.votesReceived' = Coordinator.votesReceived
    Coordinator.commitVotes' = Coordinator.commitVotes
}
pred unchangedCoordinator {
    unchangedCoordPhase
    unchangedCoordDecision
    unchangedCoordVotes
}

pred unchangedVotes {
    VotedCommit' = VotedCommit
    VotedAbort' = VotedAbort
}
pred unchangedParticipantDecision {
    ParticipantCommitted' = ParticipantCommitted
    ParticipantAborted' = ParticipantAborted
}
pred unchangedParticipant {
    unchangedVotes
    unchangedParticipantDecision
}

-- ======== Transition Relation ========

fact traces {
    always (
        coordinatorSendPrepare
        or (some n: Node | participantVote[n])
        or (some n: Node | coordinatorReceiveVote[n])
        or (some d: Decision | coordinatorDecide[d])
        or coordinatorSendDecision
        or (some n: Node | participantReceiveDecision[n])
        or stutter
    )
}

-- ======== Message Population ========

-- Constrain the Message atoms to exactly the messages needed for the protocol.
-- Each node needs 5 messages: Prepare (to it), VoteCommit/VoteAbort (from it),
-- and DecCommit/DecAbort (to it).
fact messagePopulation {
    all n: Node | {
        one m: Message | m.messageType = Prepare    and m.origin = Coordinator and m.dest = n
        one m: Message | m.messageType = VoteCommit and m.origin = n           and m.dest = Coordinator
        one m: Message | m.messageType = VoteAbort  and m.origin = n           and m.dest = Coordinator
        one m: Message | m.messageType = DecCommit  and m.origin = Coordinator and m.dest = n
        one m: Message | m.messageType = DecAbort   and m.origin = Coordinator and m.dest = n
    }
}

-- ======== Safety Properties ========

-- Agreement: all decided participants agree (all commit or all abort).
assert Agreement {
    always (all disj p1, p2: participantDecided |
        (p1 in ParticipantCommitted iff p2 in ParticipantCommitted))
}

-- Validity: a commit decision requires that every participant voted commit.
assert Validity {
    always (Coordinator.decision = Commit implies Coordinator.commitVotes = Node)
}

-- ======== Fairness ========

-- Each clause encodes a fairness assumption for one protocol action.
-- The coordinator is never forced to commit — clause 4 only requires that it
-- eventually makes _a_ decision (commit or abort). This allows the coordinator
-- to abort even when all votes are commit (e.g., timeout, policy).
--
-- Clauses use two patterns:
--   Strong: always (P implies eventually Q) — every time P holds, Q must follow.
--   Weak:   (eventually always P) implies eventually Q — Q is only required when
--           P is permanently true. If something else disables P first, no obligation.
--
-- Clause 3 uses the weak pattern so the coordinator can decide without processing
-- all available votes. This is more realistic (coordinator may timeout) and makes
-- the specification stronger: more traces are explored, so verified properties
-- hold under weaker assumptions.
--
-- | # | Pattern | Condition (enabled)                          | Guarantee (eventually)          | Why required                              |
-- |---|---------|----------------------------------------------|---------------------------------|-------------------------------------------|
-- | 1 | strong  | Coordinator is waiting                       | coordinatorSendPrepare fires    | Protocol must initiate                    |
-- | 2 | strong  | Node n hasn't voted, Prepare for n in Sent   | participantVote[n] fires        | Participants must respond to Prepare      |
-- | 3 | weak    | Coordinator voting, vote from n in Sent       | coordinatorReceiveVote[n] fires | Coordinator processes votes if it can     |
-- | 4 | strong  | Coordinator is voting                        | coord decides (commit or abort) | Coordinator must eventually decide        |
-- | 5 | strong  | Coordinator has decided                       | coordinatorSendDecision fires   | Decision must be broadcast                |
-- | 6 | strong  | Node n undecided, decision msg for n in Sent  | participantReceiveDecision[n]   | Participants must learn the outcome       |

pred fairness {
    -- 1. Coordinator sends prepare if waiting.
    always (Coordinator.phase = Waiting
            implies eventually coordinatorSendPrepare)
    -- 2. Each participant votes if it can.
    all n: Node |
        always ((n not in hasVoted and some prepareFor[n])
                implies eventually participantVote[n])
    -- 3. Coordinator receives each available vote — weak fairness.
    --    If the coordinator decides before processing a vote, the guard
    --    (phase = Voting) becomes false and the obligation vanishes.
    all n: Node |
        (eventually always (Coordinator.phase = Voting
                            and n not in Coordinator.votesReceived
                            and some voteFrom[n]))
                implies eventually coordinatorReceiveVote[n]
    -- 4. Coordinator eventually decides (commit or abort) while voting.
    --    Does NOT force commit — the coordinator freely chooses which.
    always (Coordinator.phase = Voting
            implies eventually (some d: Decision | coordinatorDecide[d]))
    -- 5. Coordinator sends decision.
    always (Coordinator.phase = Decided
            implies eventually coordinatorSendDecision)
    -- 6. Each participant receives the decision.
    all n: Node |
        always ((n not in participantDecided and some decisionFor[n])
                implies eventually participantReceiveDecision[n])
}

-- ======== Liveness Property ========

-- Termination: eventually all participants have decided.
assert Termination {
    fairness implies eventually participantDecided = Node
}

-- ======== Check Commands ========

-- Scope sizing:
--   Messages per node = 5 (Prepare + VoteCommit + VoteAbort + DecCommit + DecAbort)
--   Minimum steps for a complete trace = 3N + 3:
--     1 (sendPrepare) + N (votes) + N (receiveVotes) + 1 (decide) + 1 (sendDecision) + N (receiveDecisions)
--   Steps include generous padding for stutter and nondeterminism.

-- Scope 2: 10 msgs, min 9 steps
check Agreement   for exactly 2 Node, exactly 10 Message, 15 steps
check Validity    for exactly 2 Node, exactly 10 Message, 15 steps
check Termination for exactly 2 Node, exactly 10 Message, 15 steps

-- Scope 3: 15 msgs, min 12 steps
check Agreement   for exactly 3 Node, exactly 15 Message, 20 steps
check Validity    for exactly 3 Node, exactly 15 Message, 20 steps
check Termination for exactly 3 Node, exactly 15 Message, 20 steps

-- Scope 4: 20 msgs, min 15 steps
check Agreement   for exactly 4 Node, exactly 20 Message, 25 steps
check Validity    for exactly 4 Node, exactly 20 Message, 25 steps
check Termination for exactly 4 Node, exactly 20 Message, 25 steps

-- ======== Exploration ========

run example {
    eventually participantDecided = Node
} for exactly 2 Node, exactly 10 Message, 15 steps

run commitExample {
    eventually ParticipantCommitted = Node
} for exactly 2 Node, exactly 10 Message, 15 steps

-- ======== Event Reification ========
-- Makes events visible in the GUI visualizer.

enum Event { CoordSendPrepare, ParticipantVote, CoordReceiveVote,
             CoordDecide, CoordSendDecision,
             ParticipantReceiveDecision, Stutter }

fun _coordSendPrepare : set Event {
    { e: CoordSendPrepare | coordinatorSendPrepare }
}

fun _participantVote : Event -> Node {
    ParticipantVote -> { n: Node | participantVote[n] }
}

fun _coordReceiveVote : Event -> Node {
    CoordReceiveVote -> { n: Node | coordinatorReceiveVote[n] }
}

fun _coordDecide : Event -> Decision {
    CoordDecide -> { d: Decision | coordinatorDecide[d] }
}

fun _coordSendDecision : set Event {
    { e: CoordSendDecision | coordinatorSendDecision }
}

fun _participantReceiveDecision : Event -> Node {
    ParticipantReceiveDecision -> { n: Node | participantReceiveDecision[n] }
}

fun _stutter : set Event {
    { e: Stutter | stutter }
}

fun events : set Event {
    _coordSendPrepare + _participantVote.Node + _coordReceiveVote.Node +
    _coordDecide.Decision + _coordSendDecision +
    _participantReceiveDecision.Node + _stutter
}