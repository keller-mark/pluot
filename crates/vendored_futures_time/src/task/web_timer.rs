//! A module containing a web-compatible Timer.

use crate::time::{Duration, Instant};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use gloo_timers::callback::Timeout;

// We need to wrap the gloo_timer in an async_io-compatible Timer
// Reference: https://github.com/smol-rs/async-io/blob/master/src/lib.rs
#[derive(Debug)]
struct TimerState {
    /// Whether the timer has fired.
    fired: bool,
    /// The waker for the task.
    waker: Option<std::task::Waker>,
}

/// A Timer for usage in web environments,
/// as an alternative to async_io::Timer.
#[derive(Debug)]
pub struct Timer {
    /// The underlying timer.
    timer: Option<Timeout>,

    /// The duration.
    duration: Duration,

    /// Shared state between the future and the timer callback.
    state: Arc<Mutex<TimerState>>,
}

impl Timer {
    /// Creates a timer that emits an event once after the given duration of time.
    pub fn after(duration: Duration) -> Timer {
        Timer {
            timer: None,
            duration,
            state: Arc::new(Mutex::new(TimerState {
                fired: false,
                waker: None,
            })),
        }
    }

    /// Sets the timer to emit an event once after the given duration of time.
    pub fn set_after(&mut self, duration: Duration) {
        self.duration = duration;
        // Invalidate the existing timer so it's recreated on the next poll.
        self.timer = None;
        let mut state = self.state.lock().unwrap();
        state.fired = false;
        state.waker = None;
    }

    /// Creates a timer that emits an event once at the given instant in time.
    pub fn at(instant: Instant) -> Timer {
        Timer {
            timer: None,
            duration: instant.duration_since(*Instant::now()).into(),
            state: Arc::new(Mutex::new(TimerState {
                fired: false,
                waker: None,
            })), // TODO: check against Instant.now to see if at is in the past...
        }
    }
}

impl Future for Timer {
    type Output = Instant;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut state = this.state.lock().unwrap();

        if state.fired {
            return Poll::Ready(Instant::now());
        }

        if this.timer.is_none() {
            // The timer has not been set yet, so set it.
            let state_clone = this.state.clone();
            let timeout = Timeout::new(this.duration.as_millis() as u32, move || {
                let mut state = state_clone.lock().unwrap();
                state.fired = true;
                if let Some(waker) = state.waker.take() {
                    waker.wake();
                }
            });
            this.timer = Some(timeout);
        }

        // Store the waker so the timer can wake the task.
        state.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(timeout) = self.timer.take() {
            // The `Drop` implementation of `gloo_timers::callback::Timeout`
            // will handle cleaning up the browser timer.
            drop(timeout);
        }
    }
}
