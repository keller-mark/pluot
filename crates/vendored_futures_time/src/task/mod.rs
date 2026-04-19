//! Types and Traits for working with asynchronous tasks.

#[cfg(feature = "web")]
pub mod web_timer;

mod sleep;
mod sleep_until;

pub use sleep::{sleep, Sleep};
pub use sleep_until::{sleep_until, SleepUntil};
