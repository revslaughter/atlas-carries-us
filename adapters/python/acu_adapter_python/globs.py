"""Matches slash-separated paths against the glob patterns of a request.

``**`` matches any number of folders, ``*`` any run of characters inside one
name, and ``?`` one character. The other adapters follow the same rules.
"""

from __future__ import annotations

import functools
import re


@functools.lru_cache(maxsize=None)
def compile_glob(pattern: str) -> re.Pattern[str]:
    """Turns one glob pattern into a regular expression for a whole path."""
    out = []
    i = 0
    while i < len(pattern):
        if pattern.startswith("**/", i):
            out.append("(?:.*/)?")
            i += 3
        elif pattern.startswith("/**", i) and i + 3 == len(pattern):
            out.append("(?:/.*)?")
            i += 3
        elif pattern.startswith("**", i):
            out.append(".*")
            i += 2
        elif pattern[i] == "*":
            out.append("[^/]*")
            i += 1
        elif pattern[i] == "?":
            out.append("[^/]")
            i += 1
        else:
            out.append(re.escape(pattern[i]))
            i += 1
    return re.compile("^" + "".join(out) + "$")


def matches(path: str, patterns: list[str]) -> bool:
    """Reports whether a path matches any of the patterns."""
    return any(compile_glob(p).match(path) for p in patterns)
