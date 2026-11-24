"""Bridges to optional outside packages."""

try:
    import tomllib
except ImportError:  # pragma: no cover
    tomllib = None

import requests

from .domain import *  # noqa: F403


def fetch_book(url: str) -> Book:  # noqa: F405
    """Fetches a book's record over HTTP."""
    data = requests.get(url, timeout=5).json()
    return Book(BookId(data["id"]), data["title"])  # noqa: F405
