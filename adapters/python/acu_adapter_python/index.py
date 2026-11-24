"""Finds a project's Python files and records what each module declares.

This forms phase one of a run. It names each module from its source root,
builds the module's scope from its imports and declarations, and makes one
`Sym` per declaration. Kinds stay provisional until `classify` in
`resolve` reads the bases and decorators.
"""

from __future__ import annotations

import ast
import os
import sys
from dataclasses import dataclass, field
from typing import Optional

from .globs import matches
from .model import Binding, Module, Sym, attribute_doc, comment_doc, docstring

#: The test globs that apply when the request names none.
DEFAULT_TESTS = ["**/test_*.py", "**/*_test.py", "**/tests/**", "**/test/**", "**/conftest.py"]

#: Folder names that never hold project code.
SKIP_DIRS = {"__pycache__", "node_modules", "site-packages", "build", "dist"}


@dataclass
class Project:
    """Holds every module of one run, and the symbols that they declare."""

    #: The repository root; output paths start there.
    repo_root: str
    #: The folder that the adapter reads.
    root: str
    #: The folders where dotted module names start, relative to the root.
    source_roots: list[str]
    #: Every module, mapped or test, by dotted name.
    modules: dict[str, Module] = field(default_factory=dict)
    #: Every symbol by ID.
    symbols: dict[str, Sym] = field(default_factory=dict)
    #: Notes for the output, in plain words.
    notes: list[str] = field(default_factory=list)
    #: The distribution's name.
    distribution: str = ""
    #: Each console script's target and its line in ``pyproject.toml``.
    scripts: dict[str, tuple[str, int]] = field(default_factory=dict)
    #: The path of ``pyproject.toml``, when the root holds one.
    pyproject_path: Optional[str] = None
    #: The files that stay out, each with the reason.
    skipped: list[str] = field(default_factory=list)

    def rel_to_repo(self, abspath: str) -> str:
        """Returns a path relative to the repository root, with forward slashes."""
        return os.path.relpath(abspath, self.repo_root).replace(os.sep, "/")

    def mapped(self) -> list[Module]:
        """Returns the modules that the map shows, sorted by name."""
        return sorted((m for m in self.modules.values() if not m.is_test), key=lambda m: m.name)


def read_distribution(root: str) -> tuple[str, dict[str, tuple[str, int]], Optional[str]]:
    """Reads the distribution's name and its console scripts.

    The name comes from ``pyproject.toml``, else ``setup.cfg``, else the
    root folder. Each script maps to its ``module:function`` target and the
    line that names it.
    """
    import configparser
    import tomllib

    name = ""
    scripts: dict[str, tuple[str, int]] = {}
    path = os.path.join(root, "pyproject.toml")
    found = None
    if os.path.isfile(path):
        found = path
        with open(path, "rb") as f:
            data = tomllib.load(f)
        project = data.get("project", {})
        name = project.get("name", "") or ""
        with open(path, encoding="utf-8") as f:
            text = f.read().split("\n")
        for script, target in sorted(project.get("scripts", {}).items()):
            line = next((i + 1 for i, x in enumerate(text) if x.strip().startswith((script + " ", script + "=", '"' + script + '"'))), 1)
            scripts[script] = (str(target), line)
    cfg = os.path.join(root, "setup.cfg")
    if not name and os.path.isfile(cfg):
        parser = configparser.ConfigParser()
        parser.read(cfg, encoding="utf-8")
        name = parser.get("metadata", "name", fallback="")
    if not name:
        name = os.path.basename(os.path.abspath(root))
    return name, scripts, found


def find_files(root: str, include: list[str], exclude: list[str]) -> list[str]:
    """Lists the ``.py`` files under the root that the request allows."""
    out = []
    for dirpath, dirnames, filenames in os.walk(root):
        rel_dir = os.path.relpath(dirpath, root).replace(os.sep, "/")
        keep = []
        for d in sorted(dirnames):
            rel = d if rel_dir == "." else rel_dir + "/" + d
            if d.startswith(".") or d in SKIP_DIRS or os.path.isfile(os.path.join(dirpath, d, "pyvenv.cfg")):
                continue
            if exclude and matches(rel, exclude):
                continue
            keep.append(d)
        dirnames[:] = keep
        for f in sorted(filenames):
            if not f.endswith(".py"):
                continue
            rel = f if rel_dir == "." else rel_dir + "/" + f
            if include and not matches(rel, include):
                continue
            if exclude and matches(rel, exclude):
                continue
            out.append(rel)
    return sorted(out)


def module_name(rel: str, source_roots: list[str]) -> tuple[str, bool]:
    """Names the module of a file from the deepest source root above it."""
    best = "."
    for sr in source_roots:
        sr = sr.strip("/")
        if sr not in ("", ".") and rel.startswith(sr + "/") and (best == "." or len(sr) > len(best)):
            best = sr
    inner = rel if best == "." else rel[len(best) + 1 :]
    parts = inner[: -len(".py")].split("/")
    is_package = parts[-1] == "__init__"
    if is_package:
        parts = parts[:-1]
    return ".".join(parts) or "__init__", is_package


def build(repo_root: str, root: str, include: list[str], exclude: list[str], tests: list[str], options: dict) -> Project:
    """Reads every module under the root and records its declarations."""
    roots = list(options.get("sourceRoots") or [])
    if not roots:
        roots = ["."]
        if os.path.isdir(os.path.join(root, "src")):
            roots.append("src")
    elif "." not in roots:
        roots.append(".")
    project = Project(repo_root=repo_root, root=root, source_roots=sorted(set(roots)))
    project.distribution, project.scripts, project.pyproject_path = read_distribution(root)
    test_globs = tests or DEFAULT_TESTS
    for rel in find_files(root, include, exclude):
        abspath = os.path.join(root, rel)
        try:
            with open(abspath, encoding="utf-8") as f:
                source = f.read()
            tree = ast.parse(source, filename=rel)
        except (SyntaxError, UnicodeDecodeError, ValueError) as err:
            project.skipped.append(f"{rel} ({type(err).__name__})")
            continue
        name, is_package = module_name(rel, project.source_roots)
        is_test = matches(rel, test_globs)
        if name in project.modules:
            if is_test:
                name = "__test__." + rel[:-3].replace("/", ".")
            elif project.modules[name].is_test:
                old = project.modules.pop(name)
                old.name = "__test__." + old.rel[:-3].replace("/", ".")
                project.modules[old.name] = old
            else:
                project.skipped.append(f"{rel} (its module name {name} repeats)")
                continue
        mod = Module(
            name=name,
            path=project.rel_to_repo(abspath),
            rel=rel,
            source=source,
            tree=tree,
            is_package=is_package,
            is_test=is_test,
        )
        project.modules[name] = mod
    for mod in sorted(project.modules.values(), key=lambda m: m.name):
        Indexer(project, mod).run()
    return project


def _is_main_guard(stmt: ast.stmt) -> bool:
    """Reports whether a statement reads ``if __name__ == "__main__":``."""
    if not isinstance(stmt, ast.If) or not isinstance(stmt.test, ast.Compare):
        return False
    t = stmt.test
    if len(t.ops) != 1 or not isinstance(t.ops[0], ast.Eq):
        return False
    sides = [t.left, t.comparators[0]]
    names = [s for s in sides if isinstance(s, ast.Name) and s.id == "__name__"]
    consts = [s for s in sides if isinstance(s, ast.Constant) and s.value == "__main__"]
    return bool(names and consts)


def resolve_relative(mod: Module, level: int, name: Optional[str]) -> Optional[str]:
    """Turns a relative import's module into an absolute dotted name."""
    if level == 0:
        return name
    parts = mod.name.split(".")
    if not mod.is_package:
        parts = parts[:-1]
    if level - 1 > len(parts):
        return None
    base = parts[: len(parts) - (level - 1)]
    if name:
        base = base + name.split(".")
    return ".".join(base) if base else None


class Indexer:
    """Walks one module's tree and records its scope and declarations."""

    def __init__(self, project: Project, mod: Module) -> None:
        """Prepares to index ``mod`` into ``project``."""
        #: The project that receives the module's symbols.
        self.project = project
        #: The module that this indexer reads.
        self.mod = mod
        #: The module's source lines, for ``#:`` comments.
        self.lines = mod.source.split("\n")
        #: The symbol IDs given out so far, to spot repeats.
        self.ids: set[str] = set()

    def run(self) -> None:
        """Indexes the module's top level, then each class inside it."""
        mod = self.mod
        self.body(mod.tree.body, None)
        self.global_writes()
        for stmt in mod.tree.body:
            if isinstance(stmt, (ast.Assign, ast.AugAssign, ast.AnnAssign)):
                self.read_all(stmt)

    def global_writes(self) -> None:
        """Adds every later write of each module variable, and the writes of ``global`` scopes.

        Each write joins the writes of the variable that it rebinds, so that
        the resolver types the variable only when every write agrees. A write
        whose value the adapter cannot see, such as a ``for`` target, an
        augmented assignment or an import, joins as an opaque write (None).
        """
        mod = self.mod
        for target, value, stmt, _nested in bindings(mod.tree.body, deep=False):
            self.rebind(target, value, stmt, None, None, None)
        classes = {id(s.node): s for s in mod.symbols if s.kind == "class"}

        def visit(node: ast.AST, cls: Optional[Sym]) -> None:
            """Walks the module, keeping the class around each function."""
            for child in ast.iter_child_nodes(node):
                if isinstance(child, ast.ClassDef):
                    visit(child, classes.get(id(child)))
                elif isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    names = {n for st in _own_statements(child) if isinstance(st, ast.Global) for n in st.names}
                    if names:
                        for target, value, stmt, nested in bindings(child.body, deep=False):
                            if not nested:
                                self.rebind(target, value, stmt, names, child, cls)
                    visit(child, cls)
                else:
                    visit(child, cls)

        visit(mod.tree, None)
        self.class_name_writes()

    def class_name_writes(self) -> None:
        """Adds each write of a class attribute through the class's own name, such as ``Store.limit = 5``.

        Only writes in this module count. A write at module level keeps its
        value; a write inside a function adds an opaque write. Each write
        also joins ``self_writes``, so that the class's bases and subclasses
        share it.
        """
        for target, value, _stmt, nested in bindings(self.mod.tree.body, deep=True):
            if not (isinstance(target, ast.Attribute) and isinstance(target.value, ast.Name)):
                continue
            b = self.mod.scope.get(target.value.id)
            if b is None or b.kind != "symbol" or b.symbol is None or b.symbol.kind != "class":
                continue
            cls = b.symbol
            write = (None if nested else value, None, None)
            cls.self_writes.setdefault(target.attr, []).append(write)
            member = cls.members.get(target.attr)
            if member is not None and member.kind == "field":
                member.writes.append(write)

    def rebind(self, target: ast.expr, value: Optional[ast.expr], stmt: ast.AST, names: Optional[set[str]], fn: Optional[ast.AST], cls: Optional[Sym]) -> None:
        """Adds one write to the module variable that ``target`` names, if any.

        ``names`` limits the names to those a ``global`` statement declares;
        None means module level. The statement that declares the variable
        adds no second write.
        """
        if not isinstance(target, ast.Name) or (names is not None and target.id not in names):
            return
        b = self.mod.scope.get(target.id)
        if b is None or b.kind != "symbol" or b.symbol is None or b.symbol.kind not in {"variable", "field"}:
            return
        if b.symbol.node is stmt:
            return
        b.symbol.writes.append((value, fn, cls))

    def read_all(self, stmt: ast.stmt) -> None:
        """Reads ``__all__`` from an assignment, as a list or tuple of strings."""
        targets = stmt.targets if isinstance(stmt, ast.Assign) else [stmt.target]
        if not any(isinstance(t, ast.Name) and t.id == "__all__" for t in targets):
            return
        value = stmt.value
        if not isinstance(value, (ast.List, ast.Tuple)):
            return
        names = [e.value for e in value.elts if isinstance(e, ast.Constant) and isinstance(e.value, str)]
        if isinstance(stmt, ast.AugAssign) and self.mod.all_names is not None:
            self.mod.all_names.extend(names)
        else:
            self.mod.all_names = names

    def new_id(self, qual: str, node: ast.AST) -> str:
        """Builds a symbol ID, with ``@file:line`` when the name repeats."""
        base = "py:" + self.mod.name + "#" + qual
        if base in self.ids:
            base = base + "@" + self.mod.path + ":" + str(getattr(node, "lineno", 0))
        self.ids.add(base)
        return base

    def add(self, sym: Sym) -> Sym:
        """Records a new symbol in the module, its owner and the project."""
        self.mod.symbols.append(sym)
        self.project.symbols[sym.id] = sym
        if sym.parent is not None:
            sym.parent.members.setdefault(sym.name, sym)
        return sym

    def bind(self, name: str, binding: Binding, owner: Optional[Sym]) -> None:
        """Binds a name at the top level of the module, unless a class owns it."""
        if owner is None:
            self.mod.scope[name] = binding

    def body(self, stmts: list[ast.stmt], owner: Optional[Sym]) -> None:
        """Indexes the statements of a module body or a class body."""
        for i, stmt in enumerate(stmts):
            if owner is None and _is_main_guard(stmt):
                self.guard(stmt)
                continue
            if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef)):
                self.function(stmt, owner)
            elif isinstance(stmt, ast.ClassDef):
                self.klass(stmt, owner)
            elif isinstance(stmt, (ast.Assign, ast.AnnAssign)):
                self.assign(stmts, i, owner)
            elif isinstance(stmt, ast.TypeAlias):
                self.type_alias(stmt, owner)
            elif isinstance(stmt, (ast.Import, ast.ImportFrom)):
                if owner is None:
                    self.imports(stmt)
            elif isinstance(stmt, ast.If):
                self.body(stmt.body, owner)
                self.body(stmt.orelse, owner)
            elif isinstance(stmt, ast.Try) or isinstance(stmt, getattr(ast, "TryStar", ast.Try)):
                self.body(stmt.body, owner)
                for h in stmt.handlers:
                    self.body(h.body, owner)
                self.body(stmt.orelse, owner)
                self.body(stmt.finalbody, owner)
            elif isinstance(stmt, (ast.With, ast.AsyncWith, ast.For, ast.While)):
                self.body(stmt.body, owner)

    def imports(self, stmt: ast.stmt) -> None:
        """Adds the names that one import statement binds to the module's scope."""
        mod = self.mod
        modules = self.project.modules
        if isinstance(stmt, ast.Import):
            for alias in stmt.names:
                if alias.asname:
                    self.import_name(alias.asname, alias.name)
                else:
                    top = alias.name.split(".")[0]
                    self.import_name(top, top)
            return
        assert isinstance(stmt, ast.ImportFrom)
        base = resolve_relative(mod, stmt.level, stmt.module)
        if base is None:
            return
        for alias in stmt.names:
            if alias.name == "*":
                if base in modules:
                    mod.star_imports.append(base)
                continue
            local = alias.asname or alias.name
            if base in modules or (base + "." + alias.name) in modules:
                mod.scope[local] = Binding("from", base, alias.name)
            else:
                mod.scope[local] = Binding("external", base + "." + alias.name)

    def import_name(self, local: str, dotted: str) -> None:
        """Binds ``local`` to the module ``dotted``, of the project or outside it."""
        modules = self.project.modules
        if dotted in modules or any(m.startswith(dotted + ".") for m in modules):
            self.mod.scope[local] = Binding("module", dotted)
        else:
            self.mod.scope[local] = Binding("external", dotted)

    def guard(self, stmt: ast.If) -> None:
        """Makes the symbol of the module's main guard."""
        sym = Sym(
            id=self.new_id("__main__", stmt),
            kind="function",
            lkind="main guard",
            name="__main__",
            qual="__main__",
            module=self.mod,
            node=stmt,
            doc_raw=comment_doc(self.lines, stmt.lineno),
            line=stmt.lineno,
            end_line=stmt.end_lineno or stmt.lineno,
        )
        self.add(sym)
        self.mod.guard = sym
        for inner in stmt.body:
            if isinstance(inner, (ast.Import, ast.ImportFrom)):
                self.imports(inner)

    def qual_of(self, name: str, owner: Optional[Sym]) -> str:
        """Joins a name to the path of the symbol that owns it."""
        return owner.qual + "." + name if owner else name

    def function(self, node: ast.FunctionDef | ast.AsyncFunctionDef, owner: Optional[Sym]) -> None:
        """Records a function, a method or a property."""
        names = [_decorator_name(d) for d in node.decorator_list]
        scope = owner.members if owner else {s.name: s for s in self.mod.symbols if s.parent is None}
        prev = scope.get(node.name)
        if prev is not None and prev.node is not node:
            accessor = any(n.endswith((".setter", ".deleter", ".getter")) for n in names)
            if accessor and prev.kind == "property":
                prev.extra.append(node)
                return
            if "overload" in prev.modifiers or "overload" in names:
                if "overload" not in names:
                    prev.extra.append(prev.node)
                    prev.node = node
                    prev.decorators = list(node.decorator_list)
                    prev.line, prev.end_line = node.lineno, node.end_lineno or node.lineno
                else:
                    prev.extra.append(node)
                prev.modifiers.add("overload")
                return
        if owner is None:
            kind, lkind = "function", "function"
        elif node.name == "__init__":
            kind, lkind = "constructor", "constructor"
        elif "staticmethod" in names:
            kind, lkind = "method", "static method"
        elif "classmethod" in names:
            kind, lkind = "method", "class method"
        elif "property" in names or "abstractproperty" in names:
            kind, lkind = "property", "property"
        elif "cached_property" in names:
            kind, lkind = "property", "cached property"
        else:
            kind, lkind = "method", "method"
        qual = self.qual_of(node.name, owner)
        sym = Sym(
            id=self.new_id(qual, node),
            kind=kind,
            lkind=lkind,
            name=node.name,
            qual=qual,
            module=self.mod,
            node=node,
            parent=owner,
            decorators=list(node.decorator_list),
            doc_raw=docstring(node),
            line=node.lineno,
            end_line=node.end_lineno or node.lineno,
        )
        if isinstance(node, ast.AsyncFunctionDef):
            sym.modifiers.add("async")
        for n in names:
            if n in {"staticmethod", "classmethod", "overload", "final"}:
                sym.modifiers.add(n)
            if n in {"abstractmethod", "abstractproperty", "abstractclassmethod", "abstractstaticmethod"}:
                sym.modifiers.add("abstract")
        if _has_yield(node):
            sym.modifiers.add("generator")
        self.add(sym)
        self.bind(node.name, Binding("symbol", sym.id, symbol=sym), owner)

    def klass(self, node: ast.ClassDef, owner: Optional[Sym]) -> None:
        """Records a class and then the declarations of its body."""
        qual = self.qual_of(node.name, owner)
        sym = Sym(
            id=self.new_id(qual, node),
            kind="class",
            lkind="class",
            name=node.name,
            qual=qual,
            module=self.mod,
            node=node,
            parent=owner,
            decorators=list(node.decorator_list),
            doc_raw=docstring(node),
            bases=list(node.bases),
            line=node.lineno,
            end_line=node.end_lineno or node.lineno,
        )
        self.add(sym)
        self.bind(node.name, Binding("symbol", sym.id, symbol=sym), owner)
        self.body(node.body, sym)
        self.instance_attributes(sym, node)

    def instance_attributes(self, cls: Sym, node: ast.ClassDef) -> None:
        """Records each ``self.x = …`` in the class's methods as a field, with every write of it.

        A plain assignment in a method's own body makes the field. Every other
        binding of ``self.x`` (``+=``, a ``for`` or ``with`` target, ``del``,
        a write inside a nested function) adds an opaque write. A class
        method's ``cls.x = …`` counts as a write too, but makes no field. All
        writes go to ``cls.self_writes`` too, so that a subclass can share
        them.
        """
        methods = [s for s in node.body if isinstance(s, (ast.FunctionDef, ast.AsyncFunctionDef))]
        methods.sort(key=lambda f: (f.name != "__init__", f.lineno))
        found: dict[str, list[tuple]] = {}
        for fn in methods:
            names = [_decorator_name(d) for d in fn.decorator_list]
            if "staticmethod" in names:
                continue
            creates = "classmethod" not in names
            args = fn.args.posonlyargs + fn.args.args
            if not args:
                continue
            me = args[0].arg
            for target, value, stmt, nested in bindings(fn.body, deep=True):
                if not (isinstance(target, ast.Attribute) and isinstance(target.value, ast.Name) and target.value.id == me):
                    continue
                plain = isinstance(stmt, (ast.Assign, ast.AnnAssign)) and not nested and value is not None
                ann = stmt.annotation if isinstance(stmt, ast.AnnAssign) and stmt.target is target else None
                found.setdefault(target.attr, []).append((fn, stmt, value if plain else None, ann, plain and creates))
        for name, writes in found.items():
            cls.self_writes[name] = [(v, fn, cls) for fn, _stmt, v, _ann, _plain in writes]
            member = cls.members.get(name)
            if member is None:
                first = next((w for w in writes if w[4]), None)
                if first is None:
                    continue
                fn, stmt, value, ann, _plain = first
                qual = cls.qual + "." + name
                member = Sym(
                    id=self.new_id(qual, stmt),
                    kind="field",
                    lkind="instance attribute",
                    name=name,
                    qual=qual,
                    module=self.mod,
                    node=stmt,
                    parent=cls,
                    value=value,
                    method=fn,
                    doc_raw=comment_doc(self.lines, stmt.lineno),
                    line=stmt.lineno,
                    end_line=stmt.end_lineno or stmt.lineno,
                )
                self.add(member)
            if member.kind != "field":
                continue
            for _fn, _stmt, _v, ann, _plain in writes:
                if ann is not None and member.ann is None:
                    member.ann = ann
            member.writes.extend(cls.self_writes[name])
            # A parameter's annotation types the field only when that
            # parameter's value forms the field's one write.
            if member.ann is None and len(member.writes) == 1:
                value, fn, _cls = member.writes[0]
                if isinstance(value, ast.Name):
                    a = fn.args
                    param = next((p for p in a.posonlyargs + a.args + a.kwonlyargs if p.arg == value.id), None)
                    if param is not None and param.annotation is not None:
                        member.ann = param.annotation

    def assign(self, stmts: list[ast.stmt], i: int, owner: Optional[Sym]) -> None:
        """Records the names that a module-level or class-level assignment binds."""
        stmt = stmts[i]
        if isinstance(stmt, ast.Assign):
            targets, ann, value = stmt.targets, None, stmt.value
        else:
            assert isinstance(stmt, ast.AnnAssign)
            targets, ann, value = [stmt.target], stmt.annotation, stmt.value
        doc = attribute_doc(stmts, i, self.lines)
        for target in targets:
            names = []
            if isinstance(target, ast.Name):
                names = [(target.id, True)]
            elif isinstance(target, (ast.Tuple, ast.List)):
                names = [(e.id, False) for e in target.elts if isinstance(e, ast.Name)]
            for name, single in names:
                if name == "__all__" and owner is None:
                    continue
                scope = owner.members if owner else self.mod.scope
                # A bare annotation (``x: int``) writes nothing.
                writes = [] if value is None else [(value if single else None, None, None)]
                if name in scope:
                    existing = scope[name]
                    found = existing if owner is not None else existing.symbol if existing.kind == "symbol" else None
                    if owner is not None and found is not None and found.kind == "field":
                        found.writes.extend(writes)
                    if owner is not None or existing.kind == "symbol":
                        continue
                qual = self.qual_of(name, owner)
                sym = Sym(
                    id=self.new_id(qual, stmt),
                    kind="field" if owner else "variable",
                    lkind="class attribute" if owner else "variable",
                    name=name,
                    qual=qual,
                    module=self.mod,
                    node=stmt,
                    parent=owner,
                    ann=ann,
                    value=value if single else None,
                    writes=writes,
                    doc_raw=doc,
                    line=stmt.lineno,
                    end_line=stmt.end_lineno or stmt.lineno,
                )
                self.add(sym)
                self.bind(name, Binding("symbol", sym.id, symbol=sym), owner)

    def type_alias(self, stmt: ast.TypeAlias, owner: Optional[Sym]) -> None:
        """Records a ``type X = …`` statement as a type alias."""
        name = stmt.name.id
        qual = self.qual_of(name, owner)
        sym = Sym(
            id=self.new_id(qual, stmt),
            kind="type_alias",
            lkind="type alias",
            name=name,
            qual=qual,
            module=self.mod,
            node=stmt,
            parent=owner,
            value=stmt.value,
            line=stmt.lineno,
            end_line=stmt.end_lineno or stmt.lineno,
        )
        self.add(sym)
        self.bind(name, Binding("symbol", sym.id, symbol=sym), owner)


def _decorator_name(dec: ast.expr) -> str:
    """Returns a decorator's dotted name without its call, as in ``x.setter``."""
    if isinstance(dec, ast.Call):
        dec = dec.func
    parts = []
    while isinstance(dec, ast.Attribute):
        parts.insert(0, dec.attr)
        dec = dec.value
    if isinstance(dec, ast.Name):
        parts.insert(0, dec.id)
    name = ".".join(parts)
    for prefix in ("typing.", "typing_extensions.", "abc.", "functools."):
        if name.startswith(prefix):
            return name[len(prefix) :]
    return name


def _own_statements(fn: ast.AST):
    """Yields the statements of a function, without those of nested scopes."""
    stack = list(getattr(fn, "body", []))
    while stack:
        node = stack.pop(0)
        yield node
        for child in ast.iter_child_nodes(node):
            if isinstance(child, ast.stmt) and not isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                stack.append(child)


def _has_yield(fn: ast.AST) -> bool:
    """Reports whether a function's own body holds ``yield``."""
    stack = list(ast.iter_child_nodes(fn))
    while stack:
        node = stack.pop()
        if isinstance(node, (ast.Yield, ast.YieldFrom)):
            return True
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef, ast.Lambda)):
            continue
        stack.extend(ast.iter_child_nodes(node))
    return False


#: Top-level names of the standard library, which never count as outside packages.
STDLIB = set(sys.stdlib_module_names) | {"__future__"}


def _leaves(target: ast.expr):
    """Yields the names and attributes that one assignment target binds."""
    if isinstance(target, (ast.Tuple, ast.List)):
        for e in target.elts:
            yield from _leaves(e)
    elif isinstance(target, ast.Starred):
        yield from _leaves(target.value)
    elif isinstance(target, (ast.Name, ast.Attribute)):
        yield target


def bindings(body: list[ast.stmt], deep: bool):
    """Yields every binding in one scope: ``(target, value, statement, nested)``.

    ``value`` holds the assigned expression for a plain ``x = …`` or
    ``x: T = …`` whose target forms the whole left side, and None for every
    other form: a tuple target, ``+=``, ``for``, ``with … as``, ``:=``,
    ``import``, ``except … as``, a ``match`` capture, ``del``, and ``def``
    or ``class`` names. With ``deep``, the walk enters nested functions and
    classes too, and marks what it finds there as ``nested``.
    """
    stack: list[tuple[ast.AST, bool]] = [(n, False) for n in reversed(body)]
    while stack:
        node, nested = stack.pop()
        if isinstance(node, ast.Assign):
            for t in node.targets:
                direct = isinstance(t, (ast.Name, ast.Attribute))
                for leaf in _leaves(t):
                    yield leaf, (node.value if direct else None), node, nested
        elif isinstance(node, ast.AnnAssign):
            if node.value is not None:
                yield node.target, node.value, node, nested
        elif isinstance(node, ast.AugAssign):
            yield node.target, None, node, nested
        elif isinstance(node, (ast.For, ast.AsyncFor)):
            for leaf in _leaves(node.target):
                yield leaf, None, node, nested
        elif isinstance(node, (ast.With, ast.AsyncWith)):
            for item in node.items:
                if item.optional_vars is not None:
                    for leaf in _leaves(item.optional_vars):
                        yield leaf, None, node, nested
        elif isinstance(node, ast.NamedExpr):
            yield node.target, None, node, nested
        elif isinstance(node, (ast.Import, ast.ImportFrom)):
            for alias in node.names:
                if alias.name != "*":
                    local = alias.asname or alias.name.split(".")[0]
                    yield ast.Name(id=local, ctx=ast.Store()), None, node, nested
        elif isinstance(node, ast.ExceptHandler) and node.name:
            yield ast.Name(id=node.name, ctx=ast.Store()), None, node, nested
        elif isinstance(node, (ast.MatchAs, ast.MatchStar)) and node.name:
            yield ast.Name(id=node.name, ctx=ast.Store()), None, node, nested
        elif isinstance(node, ast.MatchMapping) and node.rest:
            yield ast.Name(id=node.rest, ctx=ast.Store()), None, node, nested
        elif isinstance(node, ast.Delete):
            for t in node.targets:
                for leaf in _leaves(t):
                    yield leaf, None, node, nested
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            yield ast.Name(id=node.name, ctx=ast.Store()), None, node, nested
            if deep:
                stack.extend((n, True) for n in reversed(node.body))
            continue
        if isinstance(node, ast.Lambda):
            if deep:
                stack.append((node.body, True))
            continue
        stack.extend((n, nested) for n in reversed(list(ast.iter_child_nodes(node))))
