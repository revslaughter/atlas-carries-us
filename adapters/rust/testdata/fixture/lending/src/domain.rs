//! The shared vocabulary: books, members, loans and the errors that every
//! part returns.

use std::fmt;

use lending_macros::Describe;

/// Identifies a book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BookId(pub u32);

/// A book on the shelves.
///
/// # Examples
///
/// ```
/// # use lending::{Book, BookId};
/// let book = Book::new(BookId(7), "Dune");
/// assert!(book.available);
/// ```
#[derive(Debug, Clone, Describe)]
pub struct Book {
    /// The book's identity.
    pub id: BookId,
    /// The title, as printed.
    pub title: String,
    /// Whether the book sits on a shelf now.
    pub available: bool,
    /// Where the book sits, if anywhere.
    shelf: Option<Shelf>,
}

impl Book {
    /// Makes an available book without a shelf.
    pub fn new(id: BookId, title: &str) -> Self {
        Book { id, title: title.to_string(), available: true, shelf: None }
    }

    /// Puts the book on a shelf.
    pub fn shelve(&mut self, shelf: Shelf) {
        self.shelf = Some(shelf);
    }

    /// Returns the shelf that holds the book.
    pub async fn locate(&self) -> Option<&Shelf> {
        self.shelf.as_ref()
    }
}

impl fmt::Display for Book {
    /// Writes the title and the identity.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (#{})", self.title, self.id.0)
    }
}

/// A place on a wall of shelves.
#[derive(Debug, Clone, PartialEq)]
pub struct Shelf {
    /// The wall's letter.
    pub wall: char,
    /// The row, from the floor up.
    pub row: u8,
}

/// A person who borrows books.
#[derive(Debug, Clone)]
pub struct Member {
    /// The member's name.
    pub name: String,
    /// The books that the member holds.
    pub held: Vec<BookId>,
}

/// A book in a member's hands, with the day it falls due.
#[derive(Debug)]
pub struct Loan {
    /// The book on loan.
    pub book: BookId,
    /// The day the loan falls due, counted from the library's start.
    pub due: u32,
}

/// Why a loan failed.
#[derive(Debug, PartialEq)]
pub enum LoanError {
    /// No book has this identity.
    NotFound(BookId),
    /// The book sits in another member's hands.
    OnLoan { book: BookId, due: u32 },
    /// The member holds too many books.
    LimitReached,
}

impl fmt::Display for LoanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoanError::NotFound(id) => write!(f, "no book {}", id.0),
            LoanError::OnLoan { book, due } => write!(f, "book {} on loan until day {due}", book.0),
            LoanError::LimitReached => f.write_str("limit reached"),
        }
    }
}

impl std::error::Error for LoanError {}

impl From<BookId> for LoanError {
    fn from(id: BookId) -> Self {
        LoanError::NotFound(id)
    }
}

/// Raw bytes of a book identity, for the barcode printer.
#[repr(C)]
pub union RawId {
    /// The identity as a number.
    pub number: u32,
    /// The identity as four bytes.
    pub bytes: [u8; 4],
}
