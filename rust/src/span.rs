/// A byte span in source code (half-open interval).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Starting byte offset (inclusive).
    pub start: usize,
    /// Ending byte offset (exclusive).
    pub end: usize,
}

impl Span {
    /// Join two spans by taking the minimum start and maximum end.
    pub fn join(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}
