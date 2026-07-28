/// Given a Future and a timeout (in ms),
/// use [`futures_time`] to throw an error if the future takes longer than
/// the specified timeout to resolve. This enables "bailing early"
/// and performing partial rendering, which is useful during interactive plotting.
#[macro_export]
macro_rules! maybe_timeout {
    ($v1:expr, $v2:expr) => {
        match $v2 {
            Some(timeout_ms) => $v1.timeout(Duration::from_millis(timeout_ms as u64)),
            // I can't seem to deal with unification of the two match arms if I simply return $v1 below.
            // For now, just set the timeout to 24 hours.
            // It would be best to avoid any timeout logic in the non-interactive case, as it can be finicky.
            None => $v1.timeout(Duration::from_millis(24 * 60 * 60 * 1000)),
        }
    };
}
