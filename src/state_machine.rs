//! Trait shared by the coordinator and participant actors.
//!
//! # Contract
//!
//! - `on_message` handles an inbound message and returns any outgoing messages.
//! - `tick` handles time progression (e.g. triggering retransmissions) and
//! returns any outgoing messages.
//! - `recover` restores state from durable storage after a (simulated) crash.
//! - `is_quiescent` reports whether the actor is in a terminal state.

use crate::types::Message;

/// A protocol actor driven by the simulator.
///
/// Each actor (coordinator, participant) implements this trait so the simulator
/// can deliver messages, advance time, crash/recover actors, and probe for
/// termination through a uniform interface.
pub trait StateMachine {
    /// Handle an inbound message and return any messages to send in response.
    fn on_message(&mut self, msg: &Message, at_time: u64) -> Vec<Message>;

    /// Called by the simulator to advance time. May produce spontaneous
    /// messages (e.g. retransmission timeouts, spontaneous abort).
    fn tick(&mut self, at_time: u64) -> Vec<Message>;

    /// Restore state from durable storage after a crash.
    fn recover(&mut self, at_time: u64);

    /// Returns `true` if this actor is in a terminal state and will not
    /// spontaneously produce new messages (i.e. `tick` will always return
    /// empty). Receiving a message may still elicit a response.
    fn is_quiescent(&self) -> bool;
}
