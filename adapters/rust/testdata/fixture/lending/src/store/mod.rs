//! Storage: the [`Store`] trait and the stores that implement it.

pub mod memory;

use crate::domain::{Book, BookId};

/// Something that can be counted.
pub trait Counted {
    /// Returns how many things it holds.
    fn count(&self) -> usize;
}

/// Keeps books. Every store implements it.
pub trait Store: Counted {
    /// The error that the store returns.
    type Error: std::fmt::Debug;

    /// The most books one store holds.
    const CAPACITY: usize;

    /// Finds a book by its identity.
    fn find(&self, id: BookId) -> Option<Book>;

    /// Saves a book, replacing any book with its identity.
    fn save(&mut self, book: Book) -> Result<(), Self::Error>;

    /// Reports whether a book exists.
    fn contains(&self, id: BookId) -> bool {
        self.find(id).is_some()
    }
}

/// Counts the available books on a list of books.
pub trait Shelved {
    /// Returns how many books on the list are available.
    fn available(&self) -> usize;
}

impl Shelved for Vec<Book> {
    fn available(&self) -> usize {
        self.iter().filter(|b| b.available).count()
    }
}
