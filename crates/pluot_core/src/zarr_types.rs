#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ZarrPeekResult {
    Pending,
    Fulfilled,
    Rejected,
}
