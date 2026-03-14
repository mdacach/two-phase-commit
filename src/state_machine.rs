//! Trait shared by the coordinator and participant actors.
//!
//! # Contract
//!
//! - `on_message` handles an inbound message and returns any outgoing messages.
//! - `tick` handles time progression (e.g. spontaneous abort) and returns any
//!   outgoing messages.
//! - `recover` restores volatile state from durable storage after a crash.
//! - `is_quiescent` reports whether the actor is in a terminal state.
//!
//! The coordinator calls `self.tick(at_time)` at the start of `on_message`, so
//! callers should **not** tick an actor separately before delivering a message.
//! The simulator follows this convention: external `Tick`/`TickAll` events call
//! `tick` directly, while `Deliver` events call `on_message` (which ticks
//! internally).

use crate::types::Message;

/// A protocol actor driven by the simulator.
///
/// Each actor (coordinator, participant) implements this trait so the simulator
/// can deliver messages, advance time, crash/recover actors, and probe for
/// termination through a uniform interface.
pub trait StateMachine {
    /// Handle an inbound message and return any messages to send in response.
    fn on_message(&mut self, msg: &Message, at_time: u64) -> Vec<Message>;

    /// Advance the actor's internal clock. May produce spontaneous messages
    /// (e.g. retransmissions, timeouts). The default implementation is a no-op.
    fn tick(&mut self, _at_time: u64) -> Vec<Message> {
        vec![]
    }

    /// Restore volatile state from durable storage after a crash.
    /// The default implementation is a no-op (stateless actor).
    fn recover(&mut self, _at_time: u64) {}

    /// Returns `true` if this actor is in a terminal state and will not
    /// spontaneously produce new messages (i.e. `tick` will always return
    /// empty).  Receiving a message may still elicit a response.
    fn is_quiescent(&self) -> bool {
        false
    }
}
