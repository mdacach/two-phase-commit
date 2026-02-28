---- MODULE TwoPhaseCommit ----

EXTENDS FiniteSets

CONSTANT Participants

VARIABLES messages,
          coordinator_phase,
          coordinator_decision,
          participant_phase,
          votes,
          decision

vars == <<messages, coordinator_phase, coordinator_decision,
          participant_phase, votes, decision>>

\* Message type constants
PREPARE     == "PREPARE"
VOTE_COMMIT == "VOTE_COMMIT"
VOTE_ABORT  == "VOTE_ABORT"
COMMIT      == "COMMIT"
ABORT       == "ABORT"

\* Phase constants
WAITING == "waiting"
VOTING  == "voting"
VOTED   == "voted"
DECIDED == "decided"
DONE    == "done"

NONE == "none"

Init ==
    /\ messages             = {}
    /\ coordinator_phase    = WAITING
    /\ coordinator_decision = NONE
    /\ participant_phase    = [p \in Participants |-> WAITING]
    /\ votes                = [p \in Participants |-> NONE]
    /\ decision             = [p \in Participants |-> NONE]

\* --- Helper: send a message ---
Send(msg) == messages' = messages \union {msg}

\* --- Coordinator Actions ---

\* At the start of the protocol, the coordinator sends a PREPARE message to
\* all participants to gather their votes.
CoordinatorSendPrepare ==
    /\ coordinator_phase = WAITING
    /\ messages' = messages \union
           {[type |-> PREPARE, source |-> "coordinator", destination |-> p]
            : p \in Participants}
    /\ coordinator_phase' = VOTING
    /\ UNCHANGED <<coordinator_decision, participant_phase, votes, decision>>

\* At any point during the voting phase, the coordinator can receive a vote
\* from a participant. In the case of a VOTE_ABORT, the coordinator immediately
\* makes its decision — it must abort the transaction, as not all nodes can commit
\* it.
\*
\* Duplicate votes are ignored, and stay in `messages` forever.
CoordinatorReceiveVote(p) ==
    /\ coordinator_phase = VOTING
    /\ votes[p] = NONE
    /\ \E msg \in messages :
        /\ msg.type \in {VOTE_COMMIT, VOTE_ABORT}
        /\ msg.source = p
        /\ msg.destination = "coordinator"
        /\ votes' = [votes EXCEPT ![p] = msg.type]
        /\ IF msg.type = VOTE_ABORT
           THEN /\ coordinator_phase' = DECIDED
                /\ coordinator_decision' = ABORT
           ELSE UNCHANGED <<coordinator_phase, coordinator_decision>>
    /\ UNCHANGED <<messages, participant_phase, decision>>

\* Coordinator decides to COMMIT after receiving VOTE_COMMITs from all nodes.
\*
\* Because a VOTE_ABORT made the coordinator ABORT the transaction (and leave
\* the VOTING phase), we only get here if we've received all VOTE_COMMITs,
\* and shall only decide to COMMIT the transaction then.
CoordinatorDecide ==
    /\ coordinator_phase = VOTING
    /\ \A p \in Participants : votes[p] = VOTE_COMMIT
    /\ coordinator_phase' = DECIDED
    /\ coordinator_decision' = COMMIT
    /\ UNCHANGED <<messages, participant_phase, votes, decision>>

\* Coordinator sends decision to all participants.
CoordinatorSendDecision ==
    /\ coordinator_phase = DECIDED
    /\ messages' = messages \union
           {[type |-> coordinator_decision, source |-> "coordinator",
             destination |-> p] : p \in Participants}
    /\ coordinator_phase' = DONE
    /\ UNCHANGED <<coordinator_decision, participant_phase, votes, decision>>

\* --- Participant Actions ---

\* Participant p votes after receiving PREPARE.
ParticipantVote(p) ==
    /\ participant_phase[p] = WAITING
    \* Messages are not removed from the set in order to simulate duplicate delivery.
    /\ \E msg \in messages :
        /\ msg.type = PREPARE
        /\ msg.destination = p
    /\ \E v \in {VOTE_COMMIT, VOTE_ABORT} :
        /\ Send([type |-> v, source |-> p, destination |-> "coordinator"])
        /\ participant_phase' = [participant_phase EXCEPT ![p] = VOTED]
    /\ UNCHANGED <<coordinator_phase, coordinator_decision, votes, decision>>

\* Participant p receives the final decision from the coordinator.
ParticipantReceiveDecision(p) ==
    \* It's possible that the decision has been made even if this 
    \* participant hasn't voted yet — in the case where another
    \* participant voted ABORT.
    /\ participant_phase[p] \in {VOTED, WAITING}
    /\ decision[p] = NONE
    /\ \E msg \in messages :
        /\ msg.type \in {COMMIT, ABORT}
        /\ msg.destination = p
        /\ decision' = [decision EXCEPT ![p] = msg.type]
        /\ participant_phase' = [participant_phase EXCEPT ![p] = DECIDED]
    /\ UNCHANGED <<messages, coordinator_phase, coordinator_decision, votes>>

\* --- Next-state relation ---

Next ==
    \/ CoordinatorSendPrepare
    \/ \E p \in Participants : CoordinatorReceiveVote(p)
    \/ CoordinatorDecide
    \/ CoordinatorSendDecision
    \/ \E p \in Participants : ParticipantVote(p)
    \/ \E p \in Participants : ParticipantReceiveDecision(p)

Spec     == Init /\ [][Next]_vars
FairSpec == Spec /\ WF_vars(Next)

\* --- Type Invariant ---

Message == [type        : {PREPARE, VOTE_COMMIT, VOTE_ABORT, COMMIT, ABORT},
            source      : Participants \union {"coordinator"},
            destination : Participants \union {"coordinator"}]

TypeCheck ==
    /\ messages \subseteq Message
    /\ coordinator_phase \in {WAITING, VOTING, DECIDED, DONE}
    /\ coordinator_decision \in {NONE, COMMIT, ABORT}
    /\ participant_phase \in [Participants -> {WAITING, VOTED, DECIDED}]
    /\ votes \in [Participants -> {NONE, VOTE_COMMIT, VOTE_ABORT}]
    /\ decision \in [Participants -> {NONE, COMMIT, ABORT}]

\* --- Safety Invariants ---

\* All participants that have decided agree on the same value.
\* Together with the "termination" liveness property, this
\* means that all participants decided, and decided on the
\* same value.
Agreement ==
    \A p1, p2 \in Participants :
        (decision[p1] /= NONE /\ decision[p2] /= NONE)
            => decision[p1] = decision[p2]

\* The coordinator decides COMMIT only if every participant voted VOTE_COMMIT.
Consistency ==
    coordinator_decision = COMMIT
        => \A p \in Participants : votes[p] = VOTE_COMMIT

\* --- Liveness Property ---

\* The protocol eventually terminates with every participant reaching a
\* decision.
Termination == <>(\A p \in Participants : decision[p] /= NONE)

====
