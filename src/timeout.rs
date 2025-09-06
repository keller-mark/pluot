/*// Use web_time for wasm32 target, std::time for others.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Duration;

#[cfg(target_arch = "wasm32")]
pub use web_time::Duration;

// We need to fork the futures-time functionality, since the original is tied to
// std::time::Duration
// Reference: https://github.com/yoshuawuyts/futures-time/blob/594c9a8a3a7eb2bd75f24cb9beb148ead07b2251/src/time/duration.rs#L18
//
// Here, I have only taken the minimal code needed for timeout functionality,
// since we do not need the other functions such as .delay, .park, etc.
// Reference: https://docs.rs/futures-time/latest/src/futures_time/future/timeout.rs.html

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

fn timeout_err(msg: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, msg)
}

pin_project! {
    /// A future that times out after a duration of time.
    ///
    /// This `struct` is created by the [`timeout`] method on [`FutureExt`]. See its
    /// documentation for more.
    ///
    /// [`timeout`]: crate::future::FutureExt::timeout
    /// [`FutureExt`]: crate::future::futureExt
    #[must_use = "futures do nothing unless polled or .awaited"]
    pub struct Timeout<F, D> {
        #[pin]
        future: F,
        #[pin]
        deadline: D,
        completed: bool,
    }
}

impl<F, D> Timeout<F, D> {
    pub(super) fn new(future: F, deadline: D) -> Self {
        Self {
            future,
            deadline,
            completed: false,
        }
    }
}

impl<F: Future, D: Future> Future for Timeout<F, D> {
    type Output = io::Result<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        assert!(!*this.completed, "future polled after completing");

        match this.future.poll(cx) {
            Poll::Ready(v) => {
                *this.completed = true;
                Poll::Ready(Ok(v))
            }
            Poll::Pending => match this.deadline.poll(cx) {
                Poll::Ready(_) => {
                    *this.completed = true;
                    Poll::Ready(Err(timeout_err("future timed out")))
                }
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

/// Conversion into a `Future`.
///
/// By implementing `Intofuture` for a type, you define how it will be
/// converted to a future. This is common for types which describe an
/// asynchronous builder of some kind.
///
/// One benefit of implementing `IntoFuture` is that your type will [work
/// with Rust's `.await` syntax](https://doc.rust-lang.org/std/keyword.await.html).
///
/// # Examples
///
/// Basic usage:
///
/// ```no_run
/// use futures_time::future::IntoFuture;
///
/// # async fn foo() {
/// let v = async { "meow" };
/// let mut fut = v.into_future();
/// assert_eq!("meow", fut.await);
/// # }
/// ```
///
/// It is common to use `IntoFuture` as a trait bound. This allows
/// the input type to change, so long as it is still a future.
/// Additional bounds can be specified by restricting on `Output`:
///
/// ```rust
/// use futures_time::future::IntoFuture;
/// async fn fut_to_string<Fut>(fut: Fut) -> String
/// where
///     Fut: IntoFuture,
///     Fut::Output: std::fmt::Debug,
/// {
///     format!("{:?}", fut.into_future().await)
/// }
/// ```
pub trait IntoFuture {
    /// The output that the future will produce on completion.
    type Output;

    /// Which kind of future are we turning this into?
    type IntoFuture: Future<Output = Self::Output>;

    /// Creates a future from a value.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use futures_time::future::IntoFuture;
    ///
    /// # async fn foo() {
    /// let v = async { "meow" };
    /// let mut fut = v.into_future();
    /// assert_eq!("meow", fut.await);
    /// # }
    /// ```

    fn into_future(self) -> Self::IntoFuture;
}

impl<F: Future> IntoFuture for F {
    type Output = F::Output;
    type IntoFuture = F;

    fn into_future(self) -> Self::IntoFuture {
        self
    }
}

/// Extend `Future` with time-based operations.
pub trait FutureExt: Future {
    /// Return an error if a future does not complete within a given time span.
    ///
    /// Typically timeouts are, as the name implies, based on _time_. However
    /// this method can time out based on any future. This can be useful in
    /// combination with channels, as it allows (long-lived) futures to be
    /// cancelled based on some external event.
    ///
    /// When a timeout is returned, the future will be dropped and destructors
    /// will be run.
    ///
    /// # Example
    ///
    /// ```
    /// use futures_time::prelude::*;
    /// use futures_time::time::{Instant, Duration};
    /// use std::io;
    ///
    /// fn main() {
    ///     async_io::block_on(async {
    ///         let res = async { "meow" }
    ///             .timeout(Duration::from_millis(50)) // shorter timeout
    ///             .await;
    ///         assert_eq!(res.unwrap_err().kind(), io::ErrorKind::TimedOut); // error
    ///
    ///         let res = async { "meow" }
    ///             .timeout(Duration::from_millis(100)) // longer timeout
    ///             .await;
    ///         assert_eq!(res.unwrap(), "meow"); // success
    ///     });
    /// }
    /// ```
    fn timeout<D>(self, deadline: D) -> Timeout<Self, D::IntoFuture>
    where
        Self: Sized,
        D: IntoFuture,
    {
        Timeout::new(self, deadline.into_future())
    }
}

impl<T> FutureExt for T where T: Future {}
*/
