//! Core types shared across the crate.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u8);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node({})", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Commit,
    Abort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActorId {
    Coordinator,
    Node(NodeId),
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActorId::Coordinator => write!(f, "Coordinator"),
            ActorId::Node(id) => write!(f, "{id}"),
        }
    }
}

/// Messages exchanged during the protocol.
///
/// The flow is:
/// ```text
/// Client ──StartTransaction──▶ Coordinator ──Prepare──▶ Participants
///                                    ◀──VoteCommit/VoteAbort──
///                              Coordinator ──DecisionCommit/DecisionAbort──▶ Participants
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    StartTransaction,
    Prepare,
    VoteCommit,
    VoteAbort,
    DecisionCommit,
    DecisionAbort,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub message_type: MessageType,
    pub from: ActorId,
    pub to: ActorId,
}
