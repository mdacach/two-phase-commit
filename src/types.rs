//! Core types shared across the crate.

use std::fmt;

/// Identifier for a [`Participant`] node.
///
/// Note that the [`Coordinator`] has its own identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u8);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node({})", self.0)
    }
}

/// A participant's vote on whether to commit or abort the transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vote {
    Commit,
    Abort,
}

/// The coordinator's final decision about the transaction: either all nodes
/// must commit the it, or all nodes must abort it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Commit,
    Abort,
}

/// Identifier for a protocol actor.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageType {
    /// External trigger that begins the protocol (client → coordinator).
    StartTransaction,
    /// Phase 1: coordinator asks each participant to vote.
    Prepare,
    /// Participant votes to commit the transaction.
    VoteCommit,
    /// Participant votes to abort the transaction.
    VoteAbort,
    /// Phase 2: coordinator broadcasts a commit decision.
    DecisionCommit,
    /// Phase 2: coordinator broadcasts an abort decision.
    DecisionAbort,
    /// Participant acknowledges receipt of the decision.
    Ack,
}

/// A message in transit between two actors.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Message {
    pub message_type: MessageType,
    pub from: ActorId,
    pub to: ActorId,
}
