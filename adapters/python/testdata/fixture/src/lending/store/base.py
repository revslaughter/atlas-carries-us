"""The storage contract that every store follows."""

from abc import ABC, abstractmethod
from typing import Optional, Protocol

from ..domain import Book, BookId


class Readable(Protocol):
    """Finds books; the library needs no more for a report."""

    def find(self, book_id: BookId) -> Optional[Book]:
        """Returns the book with this number, or None."""
        ...


class Store(ABC):
    """Saves and finds books.

    :ivar name: A name for messages.
    """

    name: str = "store"

    @abstractmethod
    def find(self, book_id: BookId) -> Optional[Book]:
        """Finds one book.

        :param book_id: The catalogue number.
        :returns: The book, or None when the store holds no such book.
        :raises KeyError: When the number has the wrong form.
        """

    @abstractmethod
    def save(self, book: Book) -> None:
        """Saves one book, replacing an older copy."""
