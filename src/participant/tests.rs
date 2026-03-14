use super::*;

fn prepare_msg(dest: NodeId) -> Message {
    Message {
        message_type: MessageType::Prepare,
        from: ActorId::Coordinator,
        to: ActorId::Node(dest),
    }
}

#[test]
fn fixed_commit_vote() {
    let mut p = Participant::with_fixed_vote(NodeId(0), Vote::Commit);
    let msgs = p.on_message(&prepare_msg(NodeId(0)), 0);
    assert_eq!(p.phase(), ParticipantPhase::Voted(Vote::Commit));
    assert_eq!(p.vote(), Some(Vote::Commit));
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].message_type, MessageType::VoteCommit);
}

#[test]
fn fixed_abort_vote() {
    let mut p = Participant::with_fixed_vote(NodeId(0), Vote::Abort);
    let msgs = p.on_message(&prepare_msg(NodeId(0)), 0);
    assert_eq!(p.phase(), ParticipantPhase::Voted(Vote::Abort));
    assert_eq!(p.vote(), Some(Vote::Abort));
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].message_type, MessageType::VoteAbort);
}

#[test]
fn receive_decision_sends_ack() {
    let mut p = Participant::with_fixed_vote(NodeId(0), Vote::Commit);
    p.on_message(&prepare_msg(NodeId(0)), 0);

    let dec = Message {
        message_type: MessageType::DecisionCommit,
        from: ActorId::Coordinator,
        to: ActorId::Node(NodeId(0)),
    };
    let msgs = p.on_message(&dec, 1);
    assert_eq!(p.decision(), Some(Decision::Commit));
    assert_eq!(p.vote(), Some(Vote::Commit));
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].message_type, MessageType::Ack);
}

#[test]
fn duplicate_prepare_resends_vote() {
    let mut p = Participant::with_fixed_vote(NodeId(0), Vote::Commit);
    p.on_message(&prepare_msg(NodeId(0)), 0);
    let msgs = p.on_message(&prepare_msg(NodeId(0)), 1);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].message_type, MessageType::VoteCommit);
}

#[test]
fn duplicate_decision_resends_ack() {
    let mut p = Participant::with_fixed_vote(NodeId(0), Vote::Commit);
    p.on_message(&prepare_msg(NodeId(0)), 0);
    let dec = Message {
        message_type: MessageType::DecisionCommit,
        from: ActorId::Coordinator,
        to: ActorId::Node(NodeId(0)),
    };
    p.on_message(&dec, 1);
    let msgs = p.on_message(&dec, 2);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].message_type, MessageType::Ack);
}

#[test]
fn recover_after_decision_restores_decided() {
    let mut p = Participant::with_fixed_vote(NodeId(0), Vote::Commit);
    p.on_message(&prepare_msg(NodeId(0)), 0);

    let dec = Message {
        message_type: MessageType::DecisionCommit,
        from: ActorId::Coordinator,
        to: ActorId::Node(NodeId(0)),
    };
    p.on_message(&dec, 1);
    assert_eq!(p.decision(), Some(Decision::Commit));

    // WAL has both vote and decision — recover restores Decided.
    p.recover(5);
    assert_eq!(p.decision(), Some(Decision::Commit));
    assert_eq!(p.vote(), Some(Vote::Commit));
}

#[test]
fn recover_after_vote_restores_voted() {
    let mut p = Participant::with_fixed_vote(NodeId(0), Vote::Commit);
    p.on_message(&prepare_msg(NodeId(0)), 0);
    assert_eq!(p.phase(), ParticipantPhase::Voted(Vote::Commit));

    // WAL has vote but no decision — recover restores Voted.
    p.recover(5);
    assert_eq!(p.phase(), ParticipantPhase::Voted(Vote::Commit));
    assert_eq!(p.decision(), None);
}

#[test]
fn recover_without_vote() {
    let mut p = Participant::with_fixed_vote(NodeId(0), Vote::Commit);
    p.recover(5);
    assert_eq!(p.phase(), ParticipantPhase::Waiting);
}
