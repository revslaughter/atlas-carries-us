"""The lending rules.

A :class:`Library` applies the rules to a :class:`~lending.store.base.Store`.
The rules:

- a member holds at most :data:`~lending.domain.MAX_LOANS` books;
- a book on loan cannot go out again.

Use it like this::

    lib = Library(MemoryStore())
    lib.borrow("b1", "ann")

.. versionadded:: 0.1
"""

from __future__ import annotations

import functools
import logging
from typing import TYPE_CHECKING, Callable

from .domain import MAX_LOANS, Book, BookId, Loan, NotAvailable, Status
from .store import Store

if TYPE_CHECKING:
    from .store.memory import MemoryStore

log = logging.getLogger(__name__)


def audited(fn: Callable) -> Callable:
    """Logs each call of a lending operation."""

    @functools.wraps(fn)
    def wrapper(*args, **kwargs):
        """Logs, then calls the operation."""
        log.info("call %s", fn.__name__)
        return fn(*args, **kwargs)

    return wrapper


class Library:
    """Applies the lending rules to one store.

    .. note:: A library holds no state but its store and its loans.
    """

    def __init__(self, store: Store) -> None:
        """Wraps a store."""
        self.store = store
        self.loans: list[Loan] = []

    @audited
    def borrow(self, book_id: BookId, member: str) -> Loan:
        """Lends a book to a member.

        :param book_id: The book to lend.
        :param member: Who borrows it.
        :returns: The new loan.
        :raises NotAvailable: When the book sits on loan already.

        .. seealso:: :meth:`give_back`
        """
        book = self.store.find(book_id)
        if book is None or book.status is Status.ON_LOAN:
            raise NotAvailable(book_id)
        if len(self.held_by(member)) >= MAX_LOANS:
            raise NotAvailable(book_id)
        book.status = Status.ON_LOAN
        self.store.save(book)
        loan = Loan(book_id, member)
        self.loans.append(loan)
        return loan

    def give_back(self, loan: Loan) -> None:
        """Ends a loan and puts the book back on the shelf."""
        book = self.store.find(loan.book)
        if book is not None:
            book.status = Status.ON_SHELF
            self.store.save(book)
        self.loans.remove(loan)

    def held_by(self, member: str) -> list[Loan]:
        """Lists the loans of one member."""
        return [x for x in self.loans if x.member == member]

    @property
    def size(self) -> int:
        """Counts the loans."""
        return len(self.loans)


class BranchLibrary(Library):
    """A library that also knows its branch.

    .. deprecated:: 0.2
       Use a :class:`Library` per branch.
    """

    def __init__(self, store: MemoryStore, branch: str) -> None:
        """Wraps a store for one branch."""
        super().__init__(store)
        self.branch = branch

    def borrow(self, book_id: BookId, member: str) -> Loan:
        """Lends a book, then logs the branch."""
        loan = super().borrow(book_id, member)
        log.info("branch %s", self.branch)
        return loan


def renew(store, book_id):
    """Renews a loan; the store's type stays unknown here."""
    return store.renewal(book_id)


def shelve[T: Book](books: list[T]) -> list[T]:
    """Sorts books by title."""
    return sorted(books, key=lambda b: b.title)
