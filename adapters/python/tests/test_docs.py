"""Unit tests of the docstring parser."""

from __future__ import annotations

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from acu_adapter_python import docs  # noqa: E402


def nothing(_name: str):
    """Resolves no name."""
    return None


def books(name: str):
    """Resolves the name Book only."""
    return "py:m#Book" if name == "Book" else None


class Styles(unittest.TestCase):
    """Each docstring style fills the doc's fields."""

    def test_google(self) -> None:
        """Google sections fill params, returns, throws and examples."""
        d = docs.parse("Does a thing.\n\nArgs:\n    x (int): The input.\n\nReturns:\n    int: The output.\n\nRaises:\n    ValueError: When x is negative.\n\nExamples:\n    >>> f(1)", nothing)
        self.assertEqual(d["format"], "docstring-google")
        self.assertEqual(d["params"], {"x": "The input."})
        self.assertEqual(d["returns"], "The output.")
        self.assertEqual(d["throws"], [{"type": "ValueError", "text": "When x is negative."}])
        self.assertEqual(d["examples"][0]["code"], ">>> f(1)")

    def test_numpy(self) -> None:
        """NumPy sections fill params, returns and throws."""
        d = docs.parse("Does a thing.\n\nParameters\n----------\nx : int\n    The input.\n\nReturns\n-------\nint\n    The output.\n\nRaises\n------\nValueError\n    When x is negative.", nothing)
        self.assertEqual(d["format"], "docstring-numpy")
        self.assertEqual(d["params"], {"x": "The input."})
        self.assertEqual(d["returns"], "The output.")
        self.assertEqual(d["throws"], [{"type": "ValueError", "text": "When x is negative."}])

    def test_sphinx(self) -> None:
        """Sphinx fields fill params, returns and throws, and drop types."""
        d = docs.parse("Does a thing.\n\n:param int x: The input,\n    never negative.\n:type x: int\n:returns: The output.\n:raises ValueError: When x is negative.", nothing)
        self.assertEqual(d["format"], "docstring-sphinx")
        self.assertEqual(d["params"], {"x": "The input, never negative."})
        self.assertEqual(d["returns"], "The output.")
        self.assertEqual(d["throws"], [{"type": "ValueError", "text": "When x is negative."}])

    def test_plain(self) -> None:
        """A docstring without sections counts as plain."""
        self.assertEqual(docs.parse("Does a thing.", nothing)["format"], "plain")


class Blocks(unittest.TestCase):
    """reStructuredText turns into blocks."""

    def test_lists_literals_and_doctests(self) -> None:
        """A list, a literal block and a doctest each make their own block."""
        d = docs.parse("Intro.\n\n- one\n- two\n\nCode::\n\n    x = 1\n\n>>> x\n1", nothing)
        kinds = [b["type"] for b in d["description"]]
        self.assertEqual(kinds, ["paragraph", "list", "paragraph", "code", "code"])
        self.assertEqual(d["description"][2]["text"], "Code:")

    def test_directives(self) -> None:
        """deprecated, versionadded and note fill their fields or a quote."""
        d = docs.parse("Intro.\n\n.. deprecated:: 2.0\n   Use g.\n\n.. versionadded:: 1.1\n\n.. note:: Mind the gap.", nothing)
        self.assertEqual(d["deprecated"], "2.0 Use g.")
        self.assertEqual(d["since"], "1.1")
        self.assertEqual(d["description"][-1], {"type": "quote", "text": "Note: Mind the gap."})

    def test_headings(self) -> None:
        """An underlined title becomes a heading."""
        d = docs.parse("Intro.\n\nUsage\n=====\n\nText.", nothing)
        self.assertEqual(d["description"][1], {"type": "heading", "level": 2, "text": "Usage"})


class Inline(unittest.TestCase):
    """Inline markup turns into the inline subset."""

    def test_roles_resolve_to_symbols(self) -> None:
        """A role that names a known symbol links to it; others become code."""
        self.assertEqual(docs.inline("See :class:`~m.Book` and :func:`g`.", lambda n: "py:m#Book" if n == "m.Book" else None), "See [Book](sym:py:m#Book) and `g`.")

    def test_literals_links_and_escapes(self) -> None:
        """Double backticks make code, a named link keeps its URL, and markup escapes."""
        self.assertEqual(docs.inline("``x*y`` and `site <https://example.com>`_ [a]", nothing), "`x*y` and [site](https://example.com) \\[a\\]")

    def test_default_role_resolves(self) -> None:
        """A single-backtick name links when it resolves."""
        self.assertEqual(docs.inline("a `Book` here", books), "a [Book](sym:py:m#Book) here")

    def test_summary_skips_abbreviations(self) -> None:
        """The summary ends at the first full stop that ends a sentence."""
        self.assertEqual(docs.summary_of("Reads files, e.g. logs. Then more."), "Reads files, e.g. logs.")


if __name__ == "__main__":
    unittest.main()
