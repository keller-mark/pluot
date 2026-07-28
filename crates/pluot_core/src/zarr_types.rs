/// Represents the result of calling a zarr status-getter function
/// (e.g., to peek at the state of a Promise via the JS bindings).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ZarrPeekResult {
    Pending,
    Fulfilled,
    Rejected,
}
