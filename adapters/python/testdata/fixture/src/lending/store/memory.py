"""A store that keeps books in a dictionary."""

from typing import Iterator, Optional, Self, overload

from ..domain import Book, BookId, Catalog
from .base import Readable, Store


class MemoryStore(Store, Readable):
    """Keeps books in memory, for tests and small runs.

    Attributes:
        books: The books by number.
    """

    def __init__(self, books: Catalog | None = None) -> None:
        """Starts with the given books, or with none.

        Args:
            books (Catalog): The books to start with.
        """
        self.books: Catalog = books or {}
        self.reads = 0

    def find(self, book_id: BookId) -> Optional[Book]:
        """Finds one book by its number.

        Args:
            book_id: The catalogue number.

        Returns:
            Book: The book, or None.
        """
        self.reads += 1
        return self.books.get(MemoryStore.key(book_id))

    def save(self, book: Book) -> None:
        """Saves one book."""
        self.books[book.id] = book

    def renewal(self, book_id: BookId) -> int:
        """Returns how many more days a renewal grants."""
        return 7

    @property
    def count(self) -> int:
        """Counts the books."""
        return len(self.books)

    @staticmethod
    def key(book_id: BookId) -> BookId:
        """Normalises a catalogue number."""
        return BookId(book_id.strip())

    @classmethod
    def empty(cls) -> Self:
        """Makes a store with no books."""
        return cls()

    def titles(self) -> Iterator[str]:
        """Yields each title, in no set order."""
        for book in self.books.values():
            yield book.title

    async def load(self, path: str) -> int:
        """Loads books from a file and counts them."""
        return 0

    @overload
    def get(self, key: BookId) -> Optional[Book]: ...
    @overload
    def get(self, key: int) -> Optional[Book]: ...
    def get(self, key):
        """Finds a book by number or by position."""
        if isinstance(key, int):
            return list(self.books.values())[key]
        return self.find(key)
