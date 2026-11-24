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

    #: What the name stands for: symbol, module, from or external.
    kind: str
    #: The symbol ID, module name or outside dotted name.
    target: str
    #: The imported name, for a ``from`` binding.
    attr: Optional[str] = None
    #: The symbol, for a ``symbol`` binding.
    symbol: Optional["Sym"] = None


@dataclass(eq=False)
class Module:
    """Holds one Python file: its dotted name, its tree and its scope."""

    #: The dotted module name; a package's ``__init__`` takes the package's name.
    name: str
    #: The path relative to the repository root.
    path: str
    #: The path relative to the adapter's root.
    rel: str
    #: The source text.
    source: str
    #: The syntax tree.
    tree: ast.Module
    #: True for an ``__init__.py`` file.
    is_package: bool
    #: True for a file that a test glob matches; the map leaves it out.
    is_test: bool
    #: The ID of the unit that holds the module.
    unit_id: str = ""
    #: The ID of the module's file container.
    file_id: str = ""
    #: The names at the module's top level.
    scope: dict[str, Binding] = field(default_factory=dict)
    #: The names in ``__all__``, when the module sets it.
    all_names: Optional[list[str]] = None
    #: The project modules that ``from x import *`` reads.
    star_imports: list[str] = field(default_factory=list)
    #: The module's symbols, in source order.
    symbols: list["Sym"] = field(default_factory=list)
    #: The symbol of the module's main guard, if any.
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

    #: The symbol's ID.
    id: str
    #: The neutral kind from the schema's list.
    kind: str
    #: The Python word for it, written to ``languageKind``.
    lkind: str
    #: The declared name.
    name: str
    #: The path inside the module, such as ``Library.borrow``.
    qual: str
    #: The module that declares it.
    module: Module
    #: The declaring syntax node.
    node: ast.AST
    #: The owning symbol, for a member.
    parent: Optional["Sym"] = None
    #: True when code outside the unit may use it.
    exported: bool = False
    #: The members of a class, by name.
    members: dict[str, "Sym"] = field(default_factory=dict)
    #: More declaring nodes: property setters and overload stubs.
    extra: list[ast.AST] = field(default_factory=list)
    #: Words such as ``async`` or ``abstract``.
    modifiers: set[str] = field(default_factory=set)
    #: The decorator expressions, as written.
    decorators: list[ast.expr] = field(default_factory=list)
    #: The cleaned docstring, or the attribute's doc.
    doc_raw: Optional[str] = None
    #: The annotation of a field or variable.
    ann: Optional[ast.expr] = None
    #: The module whose names the annotation uses.
    ann_module: Optional[Module] = None
    #: The assigned value of a field or variable.
    value: Optional[ast.expr] = None
    #: The method that assigns an instance attribute.
    method: Optional[ast.AST] = None
    #: Every write of a field or variable: its value (None when the write
    #: hides it), the function that writes it (None at module or class
    #: level), and the class of that function.
    writes: list[tuple] = field(default_factory=list)
    #: For a class: every write of ``self.<name>`` in its own methods, by
    #: name, so that its bases and subclasses can share them.
    self_writes: dict[str, list[tuple]] = field(default_factory=dict)
    #: The base class expressions of a class.
    bases: list[ast.expr] = field(default_factory=list)
    #: The first line of the declaration.
    line: int = 0
    #: The last line of the declaration.
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
    return comment_doc(lines, body[index].lineno)


def comment_doc(lines: list[str], lineno: int) -> Optional[str]:
    """Returns the run of ``#:`` comments right above line ``lineno``."""
    found: list[str] = []
    line = lineno - 2
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
