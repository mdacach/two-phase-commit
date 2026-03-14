//! Core types shared across the crate.
//!
//! These types form the protocol vocabulary: identifiers for actors, the
//! commit/abort decision value, and the messages exchanged between the
//! coordinator and participants.

use std::fmt;

/// Identifier for a participant node (0-indexed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u8);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node({})", self.0)
    }
}

/// The outcome of the two-phase commit protocol: either all nodes commit
/// the transaction, or all nodes abort it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Commit,
    Abort,
}

/// Identifies a protocol actor — either the single coordinator or one of
/// the participant nodes.
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
///                                    ◀──Ack──
/// ```
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
    /// Participant acknowledges receipt of the decision (crash-recovery extension).
    Ack,
}

/// A message in transit between two actors.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Message {
    pub message_type: MessageType,
    pub from: ActorId,
    pub to: ActorId,
}
