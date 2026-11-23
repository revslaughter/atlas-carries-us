//! A store that keeps books in memory, for tests and demos.

use std::collections::BTreeMap;

use super::{Counted, Store};
use crate::domain::{Book, BookId};

/// Keeps books in a map in memory.
#[derive(Debug, Default)]
pub struct MemoryStore {
    books: BTreeMap<BookId, Book>,
}

impl MemoryStore {
    /// Makes an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes a store that holds the given books.
    pub fn with_books(books: Vec<Book>) -> Self {
        let mut store = Self::new();
        for book in books {
            store.insert(book);
        }
        store
    }

    /// Adds one book.
    fn insert(&mut self, book: Book) {
        self.books.insert(book.id, book);
    }
}

impl Counted for MemoryStore {
    fn count(&self) -> usize {
        self.books.len()
    }
}

impl Store for MemoryStore {
    type Error = std::convert::Infallible;

    const CAPACITY: usize = 1000;

    fn find(&self, id: BookId) -> Option<Book> {
        self.books.get(&id).cloned()
    }

    fn save(&mut self, book: Book) -> Result<(), Self::Error> {
        self.insert(book);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_saved_book() {
        let mut store = MemoryStore::new();
        store.save(Book::new(BookId(1), "Dune")).unwrap();
        assert!(store.find(BookId(1)).is_some());
    }
}
