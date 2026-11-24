"""Lends books to members and keeps track of loans.

The package re-exports the parts that most callers need: :class:`Library`
and the domain types.

Example:
    >>> lib = open_library()
"""

from .domain import *  # noqa: F403
from .library import Library
from .store import MemoryStore

__all__ = ["Library", "open_library", "Book", "Status"]


def open_library() -> Library:
    """Opens a library over an empty store in memory."""
    return Library(MemoryStore.empty())
