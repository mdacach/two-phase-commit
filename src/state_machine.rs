//! Trait shared by the coordinator and participant actors.
//!
//! # Contract
//!
//! - `on_message` handles an inbound message and returns any outgoing messages.
//! - `tick` handles time progression (e.g. spontaneous abort) and returns any
//!   outgoing messages.
//!
//! The coordinator calls `self.tick(at_time)` at the start of `on_message`, so
//! callers should **not** tick an actor separately before delivering a message.
//! The simulator follows this convention: external `Tick`/`TickAll` events call
//! `tick` directly, while `Deliver` events call `on_message` (which ticks
//! internally).

use crate::types::Message;

pub trait StateMachine {
    fn on_message(&mut self, msg: &Message, at_time: u64) -> Vec<Message>;
    fn tick(&mut self, _at_time: u64) -> Vec<Message> {
        vec![]
    }
}
