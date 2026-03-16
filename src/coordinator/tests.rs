use super::*;

fn two_nodes() -> Vec<NodeId> {
    vec![NodeId(0), NodeId(1)]
}

#[test]
fn start_transaction_sends_prepare() {
    let mut coord = Coordinator::new(two_nodes(), 0, 0.0, 5);
    let start = Message {
        message_type: MessageType::StartTransaction,
        from: ActorId::Coordinator,
        to: ActorId::Coordinator,
    };
    let msgs = coord.on_message(&start, 0);
    assert_eq!(
        *coord.phase(),
        CoordinatorPhase::Voting {
            last_prepare_time: 0,
            votes: BTreeMap::new(),
        }
    );
    assert_eq!(msgs.len(), 2);
    assert!(msgs.iter().all(|m| m.message_type == MessageType::Prepare));
}

#[test]
fn all_commit_votes_enters_awaiting_acks() {
    let mut coord = Coordinator::new(two_nodes(), 0, 0.0, 5);
    coord.phase = CoordinatorPhase::Voting {
        last_prepare_time: 0,
        votes: BTreeMap::new(),
    };

    let vote0 = Message {
        message_type: MessageType::VoteCommit,
        from: ActorId::Node(NodeId(0)),
        to: ActorId::Coordinator,
    };
    coord.on_message(&vote0, 1);
    assert!(matches!(coord.phase(), CoordinatorPhase::Voting { .. }));

    let vote1 = Message {
        message_type: MessageType::VoteCommit,
        from: ActorId::Node(NodeId(1)),
        to: ActorId::Coordinator,
    };
    let msgs = coord.on_message(&vote1, 2);
    assert_eq!(
        *coord.phase(),
        CoordinatorPhase::AwaitingAcks {
            decision: Decision::Commit,
            last_decision_time: 2,
            acks: BTreeSet::new(),
        }
    );
    assert_eq!(coord.decision(), Some(Decision::Commit));
    assert_eq!(msgs.len(), 2);
    assert!(
        msgs.iter()
            .all(|m| m.message_type == MessageType::DecisionCommit)
    );
}

#[test]
fn abort_vote_enters_awaiting_acks() {
    let mut coord = Coordinator::new(two_nodes(), 0, 0.0, 5);
    coord.phase = CoordinatorPhase::Voting {
        last_prepare_time: 0,
        votes: BTreeMap::new(),
    };

    let vote = Message {
        message_type: MessageType::VoteAbort,
        from: ActorId::Node(NodeId(0)),
        to: ActorId::Coordinator,
    };
    let msgs = coord.on_message(&vote, 1);
    assert_eq!(
        *coord.phase(),
        CoordinatorPhase::AwaitingAcks {
            decision: Decision::Abort,
            last_decision_time: 1,
            acks: BTreeSet::new(),
        }
    );
    assert_eq!(coord.decision(), Some(Decision::Abort));
    assert_eq!(msgs.len(), 2);
    assert!(
        msgs.iter()
            .all(|m| m.message_type == MessageType::DecisionAbort)
    );
}

#[test]
fn tick_decided_sends_decision_messages() {
    let mut coord = Coordinator::new(two_nodes(), 0, 0.0, 5);
    coord.phase = CoordinatorPhase::Decided(Decision::Commit);

    let msgs = coord.tick(0);
    assert_eq!(
        *coord.phase(),
        CoordinatorPhase::AwaitingAcks {
            decision: Decision::Commit,
            last_decision_time: 0,
            acks: BTreeSet::new(),
        }
    );
    assert_eq!(msgs.len(), 2);
    assert!(
        msgs.iter()
            .all(|m| m.message_type == MessageType::DecisionCommit)
    );
}

#[test]
fn acks_complete_protocol() {
    let mut coord = Coordinator::new(two_nodes(), 0, 0.0, 5);
    coord.phase = CoordinatorPhase::AwaitingAcks {
        decision: Decision::Commit,
        last_decision_time: 0,
        acks: BTreeSet::new(),
    };

    let ack0 = Message {
        message_type: MessageType::Ack,
        from: ActorId::Node(NodeId(0)),
        to: ActorId::Coordinator,
    };
    coord.on_message(&ack0, 1);
    assert!(matches!(
        coord.phase(),
        CoordinatorPhase::AwaitingAcks {
            decision: Decision::Commit,
            ..
        }
    ));

    let ack1 = Message {
        message_type: MessageType::Ack,
        from: ActorId::Node(NodeId(1)),
        to: ActorId::Coordinator,
    };
    coord.on_message(&ack1, 2);
    assert_eq!(*coord.phase(), CoordinatorPhase::Done(Decision::Commit));
}

#[test]
fn retransmit_prepare_on_timeout() {
    let mut coord = Coordinator::new(two_nodes(), 0, 0.0, 5);
    coord.phase = CoordinatorPhase::Voting {
        last_prepare_time: 0,
        votes: BTreeMap::from([(NodeId(0), Vote::Commit)]),
    };

    // Before timeout: no retransmit.
    let msgs = coord.tick(4);
    assert!(msgs.is_empty());

    // At timeout: retransmit to unvoted node.
    let msgs = coord.tick(5);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].to, ActorId::Node(NodeId(1)));
    assert_eq!(msgs[0].message_type, MessageType::Prepare);
}

#[test]
fn retransmit_decision_on_timeout() {
    let mut coord = Coordinator::new(two_nodes(), 0, 0.0, 5);
    coord.phase = CoordinatorPhase::AwaitingAcks {
        decision: Decision::Commit,
        last_decision_time: 0,
        acks: BTreeSet::from([NodeId(0)]),
    };

    let msgs = coord.tick(4);
    assert!(msgs.is_empty());

    let msgs = coord.tick(5);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].to, ActorId::Node(NodeId(1)));
    assert_eq!(msgs[0].message_type, MessageType::DecisionCommit);
}

#[test]
fn recover_with_decision() {
    let mut coord = Coordinator::new(two_nodes(), 0, 0.0, 5);
    coord.durable_state.decision = Some(Decision::Commit);
    coord.recover(10);
    assert_eq!(*coord.phase(), CoordinatorPhase::Decided(Decision::Commit));

    // Next tick should send decisions.
    let msgs = coord.tick(10);
    assert_eq!(msgs.len(), 2);
    assert_eq!(
        *coord.phase(),
        CoordinatorPhase::AwaitingAcks {
            decision: Decision::Commit,
            last_decision_time: 10,
            acks: BTreeSet::new(),
        }
    );
}

#[test]
fn recover_without_decision() {
    let mut coord = Coordinator::new(two_nodes(), 0, 0.0, 5);
    coord.recover(10);
    assert_eq!(
        *coord.phase(),
        CoordinatorPhase::Voting {
            last_prepare_time: 5,
            votes: BTreeMap::new(),
        }
    );

    // Next tick should retransmit Prepare.
    let msgs = coord.tick(10);
    assert_eq!(msgs.len(), 2);
    assert!(msgs.iter().all(|m| m.message_type == MessageType::Prepare));
}
