"""Tests of the lending rules, in pytest and unittest styles."""

import unittest

import pytest

from lending import Library, open_library
from lending.domain import Book, BookId, NotAvailable
from lending.store.memory import MemoryStore


def test_borrow_lends_a_book():
    store = MemoryStore()
    store.save(Book(BookId("b1"), "Dune"))
    loan = Library(store).borrow(BookId("b1"), "ann")
    assert loan.member == "ann"


class TestStore:
    def test_finds_nothing_in_an_empty_store(self):
        assert MemoryStore.empty().find(BookId("x")) is None


class GiveBackCase(unittest.TestCase):
    def setUp(self):
        self.lib = open_library()

    def test_refuses_a_missing_book(self):
        with pytest.raises(NotAvailable):
            self.lib.borrow(BookId("none"), "ann")
