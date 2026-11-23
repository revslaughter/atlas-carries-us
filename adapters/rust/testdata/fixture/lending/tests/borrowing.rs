//! Borrowing, end to end, on a store in memory.

use lending::domain::Member;
use lending::store::memory::MemoryStore;
use lending::{Book, BookId, Library};

#[test]
fn borrows_an_available_book() {
    let store = MemoryStore::with_books(vec![Book::new(BookId(1), "Dune")]);
    let mut library = Library::new(store, 0);
    let mut member = Member { name: "Ann".into(), held: vec![] };
    let loan = library.borrow(&mut member, BookId(1)).unwrap();
    assert_eq!(loan.due, lending::MAX_LOAN_DAYS);
}
