//! The library: the lending rules applied to a store.

use crate::domain::{BookId, Loan, LoanError, Member};
use crate::rules;
use crate::store::Store;
use crate::{LoanResult, MAX_LOAN_DAYS};

/// Lends books from a store, by the [lending rules](crate::rules).
pub struct Library<S: Store> {
    store: S,
    today: u32,
}

impl<S: Store> Library<S> {
    /// Makes a library over a store, on day `today`.
    pub fn new(store: S, today: u32) -> Self {
        Library { store, today }
    }

    /// Lends a book to a member.
    ///
    /// # Errors
    ///
    /// - [`LoanError::NotFound`] when no book has the identity.
    /// - [`LoanError::LimitReached`] when the member holds too many books.
    pub fn borrow(&mut self, member: &mut Member, id: BookId) -> LoanResult<Loan> {
        let mut book = self.store.find(id).ok_or(LoanError::NotFound(id))?;
        if !rules::may_borrow(member.held.len(), 3) {
            return Err(LoanError::LimitReached);
        }
        book.available = false;
        let _ = self.store.save(book);
        member.held.push(id);
        Ok(Loan { book: id, due: self.today + MAX_LOAN_DAYS })
    }

    /// Takes a book back from a member.
    pub fn take_back(&mut self, member: &mut Member, loan: Loan) -> Result<(), LoanError> {
        let mut book = self.store.find(loan.book).ok_or_else(|| LoanError::from(loan.book))?;
        member.held.retain(|b| *b != loan.book);
        book.available = true;
        let _ = self.store.save(book);
        Ok(())
    }

    /// Returns the store.
    pub fn store(&self) -> &S {
        &self.store
    }
}
