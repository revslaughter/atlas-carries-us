"""Holds the index of a Python project: its modules, their names and symbols.

Phase one reads each module's syntax tree and records what it declares and
imports. Phase two, in `resolve`, gives each name a meaning. Nothing here
imports or runs the code under analysis.
"""

from __future__ import annotations

import ast
from dataclasses import dataclass, field
from typing import Optional


@dataclass(eq=False)
class Binding:
    """Records what one name in a module's scope stands for.

    ``kind`` holds one of ``symbol`` (a declaration of this module),
    ``module`` (``import a.b``), ``from`` (``from m import name``) and
    ``external`` (an import of an outside package).
    """

    kind: str
    target: str
    attr: Optional[str] = None
    symbol: Optional["Sym"] = None


@dataclass(eq=False)
class Module:
    """Holds one Python file: its dotted name, its tree and its scope."""

    name: str
    path: str
    rel: str
    source: str
    tree: ast.Module
    is_package: bool
    is_test: bool
    unit_id: str = ""
    file_id: str = ""
    scope: dict[str, Binding] = field(default_factory=dict)
    all_names: Optional[list[str]] = None
    star_imports: list[str] = field(default_factory=list)
    symbols: list["Sym"] = field(default_factory=list)
    guard: Optional["Sym"] = None

    def segment(self, node: Optional[ast.AST]) -> str:
        """Returns the source text of a node on one line, as written."""
        if node is None:
            return ""
        text = ast.get_source_segment(self.source, node)
        if text is None:
            text = ast.unparse(node)
        return " ".join(text.split())


@dataclass(eq=False)
class Sym:
    """Holds one declaration that becomes a symbol of the map."""

    id: str
    kind: str
    lkind: str
    name: str
    qual: str
    module: Module
    node: ast.AST
    parent: Optional["Sym"] = None
    exported: bool = False
    members: dict[str, "Sym"] = field(default_factory=dict)
    extra: list[ast.AST] = field(default_factory=list)
    modifiers: set[str] = field(default_factory=set)
    decorators: list[ast.expr] = field(default_factory=list)
    doc_raw: Optional[str] = None
    ann: Optional[ast.expr] = None
    ann_module: Optional[Module] = None
    value: Optional[ast.expr] = None
    method: Optional[ast.AST] = None
    bases: list[ast.expr] = field(default_factory=list)
    line: int = 0
    end_line: int = 0

    @property
    def dotted(self) -> str:
        """Returns the module's dotted name joined with the symbol's path."""
        return self.module.name + "." + self.qual

    @property
    def callable(self) -> bool:
        """Reports whether calls inside this symbol count as its calls."""
        return self.kind in {"function", "method", "constructor", "property"}


def attribute_doc(body: list[ast.stmt], index: int, lines: list[str]) -> Optional[str]:
    """Finds the doc of the assignment at ``body[index]``.

    A string right after the assignment counts, as Sphinx reads it. So does
    a run of ``#:`` comments right above it.
    """
    import inspect

    if index + 1 < len(body):
        nxt = body[index + 1]
        if isinstance(nxt, ast.Expr) and isinstance(nxt.value, ast.Constant) and isinstance(nxt.value.value, str):
            return inspect.cleandoc(nxt.value.value)
    stmt = body[index]
    found: list[str] = []
    line = stmt.lineno - 2
    while line >= 0 and lines[line].strip().startswith("#:"):
        found.insert(0, lines[line].strip()[2:].strip())
        line -= 1
    return "\n".join(found) if found else None


def docstring(node: ast.AST) -> Optional[str]:
    """Returns the cleaned docstring of a module, class or function."""
    try:
        return ast.get_docstring(node, clean=True)  # type: ignore[arg-type]
    except TypeError:
        return None
