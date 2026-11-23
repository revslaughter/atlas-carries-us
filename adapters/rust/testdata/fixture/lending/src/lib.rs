//! An invented lending library: books, members and loans, the rules that
//! govern a loan, and the stores that keep them.
//!
//! Start with [`Library`]: it applies the [rules] and asks a
//! [`Store`](store::Store) for the data.

pub mod domain;
pub mod library;
pub mod store;

pub use domain::{Book, BookId, LoanError};
pub use library::Library;

/// The longest loan, in days.
pub const MAX_LOAN_DAYS: u32 = 21;

/// How many loans the library has made since it started.
pub static mut LOANS_MADE: u64 = 0;

/// A result whose error is a [`LoanError`].
pub type LoanResult<T> = Result<T, LoanError>;

/// Writes a log line with the library's prefix.
#[macro_export]
macro_rules! log_line {
    ($($arg:tt)*) => {
        format!("lending: {}", format!($($arg)*))
    };
}

/// Returns the version of the lending rules.
#[deprecated(since = "0.1.0", note = "read MAX_LOAN_DAYS instead")]
pub fn rules_version() -> u32 {
    rules::VERSION
}

pub mod rules {
    //! The rules of a loan, in one place.

    /// The version of these rules.
    pub const VERSION: u32 = 2;

    /// Reports whether a member may borrow one more book.
    ///
    /// A member may hold at most `limit` books. See [`crate::MAX_LOAN_DAYS`]
    /// for the length of a loan.
    pub fn may_borrow(held: usize, limit: usize) -> bool {
        held < limit && within_hours()
    }

    /// Reports whether the desk is open.
    fn within_hours() -> bool {
        true
    }
}

extern "C" {
    /// Returns the absolute value, from the C library.
    fn abs(x: i32) -> i32;
}

/// Returns the distance between two shelf positions.
pub fn shelf_distance(a: i32, b: i32) -> i32 {
    // SAFETY: abs has no preconditions.
    unsafe { abs(a - b) }
}
