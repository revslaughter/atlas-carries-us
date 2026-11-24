"""Gives names a meaning: phase two of a run.

The resolver follows Python's scope rules (local, enclosing function,
module, imports) and knows a value's type from an annotation, a constructor
call, a return annotation, ``self``, ``cls`` and ``super()``. It never runs
the code. A value takes one of these forms:

- ``("module", name)``: a module of the project;
- ``("class", sym)``: a class of the project itself;
- ``("inst", sym)``: an instance of a class of the project;
- ``("func", sym)``: a function, method or property of the project;
- ``("super", sym)``: ``super()`` inside a method of the class ``sym``;
- ``("ext", dotted)``: something from outside the project, by dotted name;
- ``None``: unknown.
"""

from __future__ import annotations

import ast
import builtins
from typing import Optional

from .index import Project
from .model import Binding, Module, Sym

#: A resolved value: a tuple whose first item names its form, or None.
Value = Optional[tuple]

#: Names of the built-in exception classes.
BUILTIN_EXCEPTIONS = {n for n in dir(builtins) if isinstance(getattr(builtins, n), type) and issubclass(getattr(builtins, n), BaseException)}

#: Wrappers in annotations whose first argument carries the type.
_WRAPPERS = {"Optional", "Annotated", "Final", "ClassVar", "Required", "NotRequired", "ReadOnly", "Type", "type"}

#: Kinds of symbol that a type annotation may name.
TYPE_KINDS = {"class", "struct", "interface", "enum", "type_alias", "type"}

#: The module prefixes whose names count as typing's own.
_TYPING = ("typing.", "typing_extensions.")


def typing_name(value: Value) -> str:
    """Returns the name inside ``typing`` that an outside value names, or ``""``."""
    if value and value[0] == "ext":
        for prefix in _TYPING:
            if value[1].startswith(prefix):
                return value[1][len(prefix) :]
        if value[1].startswith("builtins."):
            return value[1][len("builtins.") :]
    return ""


class Ctx:
    """Holds one scope: a module's top level, or a function inside it."""

    def __init__(self, resolver: "Resolver", module: Module, func: Optional[ast.AST] = None, cls: Optional[Sym] = None, parent: Optional["Ctx"] = None) -> None:
        """Builds the scope of ``func`` (or of the module when ``func`` is None)."""
        #: The resolver that owns this scope.
        self.r = resolver
        #: The module that holds the scope.
        self.module = module
        #: The function of the scope; None for a module's top level.
        self.func = func
        #: The class whose method this scope is, if any.
        self.cls = cls
        #: The enclosing scope.
        self.parent = parent
        #: How the function binds each local name.
        self.locals: dict[str, list[tuple]] = {}
        #: The names that ``global`` or ``nonlocal`` send outward.
        self.outer: set[str] = set()
        #: Each name's value, once looked up.
        self.cache: dict[str, Value] = {}
        if func is not None:
            self._collect()

    def _collect(self) -> None:
        """Records how the function binds each local name."""
        fn = self.func
        args = fn.args  # type: ignore[union-attr]
        positional = args.posonlyargs + args.args
        for i, a in enumerate(positional):
            self.locals.setdefault(a.arg, []).append(("param", a.annotation, i))
        for a in args.kwonlyargs:
            self.locals.setdefault(a.arg, []).append(("param", a.annotation, -1))
        for a in (args.vararg, args.kwarg):
            if a is not None:
                self.locals.setdefault(a.arg, []).append(("other",))
        if isinstance(fn, ast.Lambda):
            return
        # The names that a nested scope declares nonlocal, and so may rebind.
        rebound: set[str] = set()
        stack = list(fn.body)  # type: ignore[union-attr]
        while stack:
            node = stack.pop()
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                self.locals.setdefault(node.name, []).append(("other",))
                rebound.update(n for sub in ast.walk(node) if isinstance(sub, ast.Nonlocal) for n in sub.names)
                continue
            if isinstance(node, ast.Lambda):
                continue
            if isinstance(node, (ast.ListComp, ast.SetComp, ast.DictComp, ast.GeneratorExp)):
                # A comprehension keeps its loop names to itself, but := inside it binds a name of this scope.
                for sub in ast.walk(node):
                    if isinstance(sub, ast.NamedExpr):
                        self._target(sub.target, None)
                continue
            if isinstance(node, (ast.Global, ast.Nonlocal)):
                self.outer.update(node.names)
            elif isinstance(node, ast.Assign):
                for t in node.targets:
                    self._target(t, node.value)
            elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
                self.locals.setdefault(node.target.id, []).append(("annot", node.annotation))
            elif isinstance(node, ast.AugAssign):
                self._target(node.target, None)
            elif isinstance(node, ast.Delete):
                for t in node.targets:
                    self._target(t, None)
            elif isinstance(node, ast.NamedExpr):
                self._target(node.target, node.value)
            elif isinstance(node, (ast.For, ast.AsyncFor)):
                self._target(node.target, None)
            elif isinstance(node, (ast.With, ast.AsyncWith)):
                for item in node.items:
                    if item.optional_vars is not None:
                        self._target(item.optional_vars, None)
            elif isinstance(node, ast.ExceptHandler) and node.name:
                self.locals.setdefault(node.name, []).append(("other",))
            elif isinstance(node, ast.Import):
                for alias in node.names:
                    local = alias.asname or alias.name.split(".")[0]
                    self.locals.setdefault(local, []).append(("import", alias.name if alias.asname else local, None, 0))
            elif isinstance(node, ast.ImportFrom):
                for alias in node.names:
                    if alias.name != "*":
                        self.locals.setdefault(alias.asname or alias.name, []).append(("import", node.module, alias.name, node.level))
            elif isinstance(node, (ast.MatchAs, ast.MatchStar)) and node.name:
                self.locals.setdefault(node.name, []).append(("other",))
            elif isinstance(node, ast.MatchMapping) and node.rest:
                self.locals.setdefault(node.rest, []).append(("other",))
            stack.extend(ast.iter_child_nodes(node))
        for name in self.outer:
            self.locals.pop(name, None)
        # A nested write through nonlocal hides its value from this scope.
        for name in rebound:
            if name in self.locals:
                self.locals[name].append(("other",))

    def _target(self, target: ast.expr, value: Optional[ast.expr]) -> None:
        """Records one assignment target; only a plain name keeps its value."""
        if isinstance(target, ast.Name):
            self.locals.setdefault(target.id, []).append(("assign", value) if value is not None else ("other",))
        elif isinstance(target, (ast.Tuple, ast.List)):
            for e in target.elts:
                self._target(e, None)
        elif isinstance(target, ast.Starred):
            self._target(target.value, None)

    def lookup(self, name: str) -> Value:
        """Returns the value that a name holds in this scope."""
        if name in self.cache:
            return self.cache[name]
        self.cache[name] = None
        value = self._lookup(name)
        self.cache[name] = value
        return value

    def _lookup(self, name: str) -> Value:
        """Looks a name up without the cache: local, enclosing, module, built-in."""
        if name in self.locals:
            return self._local(name, self.locals[name])
        if self.parent is not None:
            return self.parent.lookup(name)
        return self.r.global_name(self.module, name)

    def _local(self, name: str, defs: list[tuple]) -> Value:
        """Infers a local's value from all the ways the function binds it."""
        r = self.r
        for d in defs:
            if d[0] == "annot":
                return r.annotation_value(d[1], self.module)
        for d in defs:
            if d[0] == "param":
                if d[1] is not None:
                    return r.annotation_value(d[1], self.module)
                if d[2] == 0 and self.cls is not None:
                    names = r.decorator_names(self.func)
                    if "staticmethod" in names:
                        return None
                    if "classmethod" in names or name == "cls":
                        return ("class", self.cls)
                    return ("inst", self.cls)
                return None
        if any(d[0] == "other" for d in defs):
            return None
        values = []
        for d in defs:
            if d[0] == "import":
                values.append(r.import_value(self.module, d[1], d[2], d[3]))
            else:
                values.append(r.value(d[1], self))
        first = values[0]
        if first is None or any(not _same(v, first) for v in values[1:]):
            return None
        return first


def _same(a: Value, b: Value) -> bool:
    """Compares two values; symbols compare by identity."""
    if a is None or b is None:
        return a is b
    return a[0] == b[0] and a[1] is b[1] if not isinstance(a[1], str) else a == b


class Resolver:
    """Answers what names, expressions and annotations stand for in a project."""

    def __init__(self, project: Project) -> None:
        """Prepares caches for one project."""
        #: The project whose names it resolves.
        self.p = project
        #: The scope of each module's top level, by name.
        self.module_ctx: dict[str, Ctx] = {}
        #: The scope of each function, by the identity of its node.
        self.func_ctx: dict[int, Ctx] = {}
        self._mro: dict[str, list[Sym]] = {}
        self._busy: set[str] = set()
        self._var: dict[str, Value] = {}

    def ctx_of_module(self, mod: Module) -> Ctx:
        """Returns the scope of a module's top level."""
        if mod.name not in self.module_ctx:
            self.module_ctx[mod.name] = Ctx(self, mod)
        return self.module_ctx[mod.name]

    def ctx_of(self, fn: ast.AST, mod: Module, cls: Optional[Sym] = None, parent: Optional[Ctx] = None) -> Ctx:
        """Returns the scope of a function, built once."""
        key = id(fn)
        if key not in self.func_ctx:
            self.func_ctx[key] = Ctx(self, mod, fn, cls, parent or self.ctx_of_module(mod))
        return self.func_ctx[key]

    def ctx_of_symbol(self, sym: Sym) -> Ctx:
        """Returns the scope inside a callable symbol's body."""
        if sym.lkind == "main guard":
            return self.ctx_of_module(sym.module)
        cls = sym.parent if sym.parent is not None and sym.parent.kind != "function" else None
        return self.ctx_of(sym.node, sym.module, cls)

    def decorator_names(self, fn: Optional[ast.AST]) -> set[str]:
        """Returns the plain names of a function's decorators."""
        from .index import _decorator_name

        return {_decorator_name(d) for d in getattr(fn, "decorator_list", [])}

    # Names and modules.

    def global_name(self, mod: Module, name: str) -> Value:
        """Returns the value of a name at a module's top level, else a built-in."""
        found = self.member(mod.name, name)
        if found is not None:
            return found
        if hasattr(builtins, name):
            return ("ext", "builtins." + name)
        return None

    def member(self, modname: str, name: str, depth: int = 0) -> Value:
        """Returns the value of ``modname.name``, following imports and re-exports."""
        if depth > 20:
            return None
        mod = self.p.modules.get(modname)
        if mod is None:
            return None
        b = mod.scope.get(name)
        if b is not None:
            return self.binding_value(mod, b, depth)
        for star in mod.star_imports:
            other = self.p.modules.get(star)
            if other is not None and self.exports(other, name):
                return self.member(star, name, depth + 1)
        if modname + "." + name in self.p.modules:
            return ("module", modname + "." + name)
        return None

    def member_symbol(self, modname: str, name: str, depth: int = 0) -> Optional[Sym]:
        """Returns the symbol that ``modname.name`` binds, following imports."""
        mod = self.p.modules.get(modname)
        if mod is None or depth > 20:
            return None
        b = mod.scope.get(name)
        if b is None:
            for star in mod.star_imports:
                other = self.p.modules.get(star)
                if other is not None and self.exports(other, name):
                    return self.member_symbol(star, name, depth + 1)
            return None
        if b.kind == "symbol":
            return b.symbol
        if b.kind == "from":
            return self.member_symbol(b.target, b.attr or "", depth + 1)
        return None

    def exports(self, mod: Module, name: str) -> bool:
        """Reports whether ``from mod import *`` brings in ``name``."""
        if mod.all_names is not None:
            return name in mod.all_names
        return not name.startswith("_") and (name in mod.scope)

    def binding_value(self, mod: Module, b: Binding, depth: int = 0) -> Value:
        """Returns the value that one binding of a module's scope holds."""
        if b.kind == "symbol":
            return self.symbol_value(b.symbol)  # type: ignore[arg-type]
        if b.kind == "module":
            return ("module", b.target)
        if b.kind == "external":
            return ("ext", b.target)
        if b.kind == "from":
            if b.target == mod.name and b.attr in mod.scope and mod.scope[b.attr] is b:
                return None
            found = self.member(b.target, b.attr or "", depth + 1)
            if found is None and b.target + "." + (b.attr or "") in self.p.modules:
                return ("module", b.target + "." + (b.attr or ""))
            return found
        return None

    def import_value(self, mod: Module, module: Optional[str], name: Optional[str], level: int) -> Value:
        """Returns the value that an import inside a function binds."""
        from .index import resolve_relative

        if name is None:
            if module and module in self.p.modules or any(m.startswith((module or "") + ".") for m in self.p.modules):
                return ("module", module)
            return ("ext", module)
        base = resolve_relative(mod, level, module)
        if base is None:
            return None
        if base in self.p.modules or base + "." + name in self.p.modules:
            return self.member(base, name) or (("module", base + "." + name) if base + "." + name in self.p.modules else None)
        return ("ext", base + "." + name)

    def symbol_value(self, sym: Sym) -> Value:
        """Returns the value that a name bound to a symbol holds."""
        if sym.kind in TYPE_KINDS:
            return ("class", sym)
        if sym.kind in {"function", "method", "constructor", "property"}:
            return ("func", sym)
        return self.variable_value(sym)

    def variable_value(self, sym: Sym) -> Value:
        """Infers a variable's or field's value from its annotation, or from writes that all agree."""
        if sym.id in self._var:
            return self._var[sym.id]
        if sym.id in self._busy:
            return None
        self._busy.add(sym.id)
        value: Value = None
        if sym.parent is not None and sym.parent.kind == "enum" and sym.kind == "constant":
            value = ("inst", sym.parent)
        elif sym.ann is not None:
            value = self.annotation_value(sym.ann, sym.module)
        elif sym.writes:
            # As for a local name: a type holds only when every write agrees.
            values = []
            for expr, fn, cls in sym.writes:
                if expr is None:
                    values = [None]
                    break
                ctx = self.ctx_of(fn, sym.module, cls) if fn is not None else self.ctx_of_module(sym.module)
                values.append(self.value(expr, ctx))
            first = values[0]
            if first is not None and all(_same(v, first) for v in values[1:]):
                value = first
        self._busy.discard(sym.id)
        self._var[sym.id] = value
        return value

    # Expressions.

    def value(self, expr: Optional[ast.expr], ctx: Ctx) -> Value:
        """Returns the value of an expression in a scope."""
        if expr is None:
            return None
        if isinstance(expr, ast.Name):
            return ctx.lookup(expr.id)
        if isinstance(expr, ast.Attribute):
            return self.attribute(self.value(expr.value, ctx), expr.attr)
        if isinstance(expr, ast.Call):
            if isinstance(expr.func, ast.Name) and expr.func.id == "super" and ctx.cls is not None and ctx.lookup("super") == ("ext", "builtins.super"):
                return ("super", ctx.cls)
            return self.call_result(self.value(expr.func, ctx))
        if isinstance(expr, ast.Await):
            return self.value(expr.value, ctx)
        return None

    def attribute(self, base: Value, name: str) -> Value:
        """Returns the value of ``base.name``."""
        if base is None:
            return None
        kind = base[0]
        if kind == "module":
            return self.member(base[1], name)
        if kind == "ext":
            return ("ext", base[1] + "." + name)
        if kind in {"class", "inst"}:
            cls = self.alias_target(base[1])
            if cls is None:
                return None
            found = self.lookup_mro(cls, name)
            if found is None:
                return ("ext", "?") if kind == "inst" or self.has_outside_base(cls) else None
            if found.kind in TYPE_KINDS:
                return ("class", found)
            if found.kind == "property":
                if kind == "class":
                    return ("func", found)
                return self.result_of(found)
            if found.kind in {"function", "method", "constructor"}:
                return ("func", found)
            return self.variable_value(found)
        if kind == "super":
            mro = self.mro(base[1])
            for k in mro[1:]:
                if name in k.members:
                    m = k.members[name]
                    return ("func", m) if m.callable else self.variable_value(m)
            return ("ext", "?")
        return None

    def alias_target(self, sym: Sym) -> Optional[Sym]:
        """Follows a type alias or new type to the class it names."""
        seen = 0
        while sym.kind in {"type_alias", "type"} and seen < 10:
            v = self.annotation_value(sym.value if sym.kind == "type_alias" else _newtype_base(sym), sym.module)
            if v is None or v[0] not in {"inst", "class"}:
                return None
            sym = v[1]
            seen += 1
        return sym

    def call_result(self, callee: Value) -> Value:
        """Returns the value that a call of ``callee`` gives back."""
        if callee is None:
            return None
        if callee[0] == "class":
            return ("inst", callee[1])
        if callee[0] == "func":
            return self.result_of(callee[1])
        if callee[0] == "ext":
            return ("ext", callee[1] + "()")
        return None

    def result_of(self, fn: Sym) -> Value:
        """Returns the value of a function's result, from its return annotation."""
        returns = getattr(fn.node, "returns", None)
        if returns is None:
            return None
        if isinstance(returns, ast.Name) and returns.id == "Self" and fn.parent is not None:
            return ("inst", fn.parent)
        return self.annotation_value(returns, fn.module)

    # Annotations.

    def annotation_value(self, ann: Optional[ast.expr], mod: Module) -> Value:
        """Returns the value that an annotation promises: an instance of one class."""
        if ann is None:
            return None
        if isinstance(ann, ast.Constant) and isinstance(ann.value, str):
            parsed = _parse_annotation(ann.value)
            return self.annotation_value(parsed, mod) if parsed is not None else None
        ctx = self.ctx_of_module(mod)
        if isinstance(ann, (ast.Name, ast.Attribute)):
            v = self.value(ann, ctx)
            if v is not None and v[0] == "class":
                return ("inst", v[1])
            if v is not None and v[0] == "ext":
                return ("ext", v[1] + "()")
            return None
        if isinstance(ann, ast.Subscript):
            head = typing_name(self.value(ann.value, ctx))
            args = ann.slice.elts if isinstance(ann.slice, ast.Tuple) else [ann.slice]
            if head in {"type", "Type"}:
                inner = self.annotation_value(args[0], mod)
                return ("class", inner[1]) if inner and inner[0] == "inst" else None
            if head in _WRAPPERS and args:
                return self.annotation_value(args[0], mod)
            if head == "Union":
                return self._one_of(args, mod)
            return None
        if isinstance(ann, ast.BinOp) and isinstance(ann.op, ast.BitOr):
            return self._one_of([ann.left, ann.right], mod)
        return None

    def _one_of(self, parts: list[ast.expr], mod: Module) -> Value:
        """Returns the one type of a union that leaves out ``None``."""
        rest = [p for p in parts if not (isinstance(p, ast.Constant) and p.value is None)]
        if len(rest) != 1:
            return None
        return self.annotation_value(rest[0], mod)

    def refs(self, ann: Optional[ast.expr], mod: Module) -> list[str]:
        """Lists the IDs of the project's types that an annotation names."""
        out: list[str] = []
        ctx = self.ctx_of_module(mod)

        def visit(node: ast.AST) -> None:
            """Collects the types that one part of the annotation names."""
            if isinstance(node, ast.Constant) and isinstance(node.value, str):
                parsed = _parse_annotation(node.value)
                if parsed is not None:
                    visit(parsed)
                return
            if isinstance(node, (ast.Name, ast.Attribute)):
                v = self.value(node, ctx)
                if v is not None and v[0] == "class" and not v[1].module.is_test:
                    out.append(v[1].id)
                    return
                if isinstance(node, ast.Name):
                    return
            for child in ast.iter_child_nodes(node):
                visit(child)

        if ann is not None:
            visit(ann)
        return sorted(set(out))

    # Classes.

    def base_values(self, cls: Sym) -> list[Value]:
        """Returns the value of each base class expression of a class."""
        ctx = self.ctx_of_module(cls.module)
        out = []
        for b in cls.bases:
            if isinstance(b, ast.Subscript):
                b = b.value
            out.append(self.value(b, ctx))
        return out

    def project_bases(self, cls: Sym) -> list[Sym]:
        """Returns the bases of a class that the project declares."""
        out = []
        for v in self.base_values(cls):
            if v is not None and v[0] == "class" and v[1] is not cls:
                target = self.alias_target(v[1])
                if target is not None and target.kind not in {"type_alias", "type"}:
                    out.append(target)
        return out

    def outside_bases(self, cls: Sym) -> list[str]:
        """Returns the dotted names of the bases from outside the project."""
        return [v[1] for v in self.base_values(cls) if v is not None and v[0] == "ext"]

    def has_outside_base(self, cls: Sym) -> bool:
        """Reports whether any class in the method resolution order comes from outside."""
        return any(self.outside_bases(k) for k in self.mro(cls))

    def mro(self, cls: Sym) -> list[Sym]:
        """Returns the method resolution order of the project's classes (C3)."""
        if cls.id in self._mro:
            return self._mro[cls.id]
        self._mro[cls.id] = [cls]
        seqs = [list(self.mro(b)) for b in self.project_bases(cls)] + [self.project_bases(cls)]
        out = [cls]
        while True:
            seqs = [s for s in seqs if s]
            if not seqs:
                break
            for s in seqs:
                head = s[0]
                if not any(head in t[1:] for t in seqs):
                    break
            else:
                for s in seqs:
                    for k in s:
                        if k not in out:
                            out.append(k)
                break
            out.append(head)
            for s in seqs:
                if s and s[0] is head:
                    s.pop(0)
        self._mro[cls.id] = out
        return out

    def lookup_mro(self, cls: Sym, name: str) -> Optional[Sym]:
        """Finds the member ``name`` along a class's method resolution order."""
        for k in self.mro(cls):
            if name in k.members:
                return k.members[name]
        return None

    def framework_class(self, value: Value, names: set[str]) -> bool:
        """Reports whether a value holds an instance of one of the named outside classes.

        A project class counts when its own dotted name, or the name of one of
        its outside bases, appears in ``names``. So the check works on the
        framework's own code too.
        """
        if value is None:
            return False
        if value[0] == "ext":
            return value[1].endswith("()") and value[1][:-2] in names
        if value[0] == "inst":
            for k in self.mro(value[1]):
                if k.dotted in names or any(b in names for b in self.outside_bases(k)):
                    return True
        return False

    def class_names(self, value: Value) -> set[str]:
        """Returns the dotted names that a class value can go by."""
        if value is None:
            return set()
        if value[0] == "ext":
            return {value[1]}
        if value[0] == "class":
            out = set()
            for k in self.mro(value[1]):
                out.add(k.dotted)
                out.update(self.outside_bases(k))
            return out
        return set()


def _newtype_base(sym: Sym) -> Optional[ast.expr]:
    """Returns the base type of ``X = NewType("X", Base)``."""
    v = sym.value
    if isinstance(v, ast.Call) and len(v.args) >= 2:
        return v.args[1]
    return None


def _parse_annotation(text: str) -> Optional[ast.expr]:
    """Parses a string annotation, or returns None when it does not parse."""
    try:
        return ast.parse(text.strip(), mode="eval").body
    except SyntaxError:
        return None
