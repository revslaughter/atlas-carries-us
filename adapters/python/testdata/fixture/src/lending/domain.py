"""The shared vocabulary: books, members, loans and errors.

Notes
-----
Every other module speaks these words.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Final, NamedTuple, NewType, TypeAlias, TypedDict, TypeVar

__all__ = ["Book", "BookId", "Catalog", "LOAN_DAYS", "Loan", "LendingError", "MAX_LOANS", "MemberRecord", "NotAvailable", "Shelf", "Status"]

#: How many books one member may hold at once.
MAX_LOANS: Final = 3

LOAN_DAYS = 14
"""How many days a loan lasts."""

BookId = NewType("BookId", str)
"""A book's catalogue number."""

T = TypeVar("T")


class Status(Enum):
    """Says where a book is."""

    ON_SHELF = "on shelf"
    ON_LOAN = "on loan"


@dataclass
class Book:
    """One copy of a book.

    Parameters
    ----------
    id : BookId
        The catalogue number.
    title : str
        The title, as printed.

    See Also
    --------
    Loan : The record of a book on loan.
    """

    id: BookId
    title: str
    status: Status = Status.ON_SHELF
    tags: list[str] = field(default_factory=list)


class Loan(NamedTuple):
    """Records that a member holds a book."""

    book: BookId
    member: str
    days: int = LOAN_DAYS


class MemberRecord(TypedDict):
    """A member as the old system exported it."""

    name: str
    loans: list[Loan]


class LendingError(Exception):
    """Reports a broken lending rule."""


class NotAvailable(LendingError):
    """Reports a book that sits on loan already."""


Catalog: TypeAlias = dict[BookId, Book]
"""All books by catalogue number."""

type Shelf = list[Book]
