"""Builds the data format's document from an indexed project.

`extract` runs phase two: it classifies the symbols, writes the containers
and symbols, resolves the edges and the tests, runs the entry-point
plugins, and sorts every list.
"""

from __future__ import annotations

import ast
import builtins
import io
import os
from typing import Optional

from . import docs as docparse
from . import plugins
from .index import STDLIB, Project, build, resolve_relative
from .model import Module, Sym
from .resolve import BUILTIN_EXCEPTIONS, TYPE_KINDS, Ctx, Resolver, typing_name

#: The adapter's name and version, as `meta` reports them.
NAME, VERSION = "acu-adapter-python", "0.1.0"

#: The limits of every run, which the output's notes and the help text list.
LIMITS = [
    "Calls through values whose type the adapter cannot tell do not appear, unless one method of the project alone carries the name; such calls count as heuristic.",
    "A call through a base class or a protocol points at that class's method.",
    "Monkey-patching, setattr, getattr with a computed name and __getattr__ hide what code runs; the map shows only names written in the code.",
    "A decorator that changes a signature leaves the signature as the def writes it; metaclasses and class decorators add no members.",
    "Code at module level, outside functions, makes no calls on the map, except inside a main guard.",
    "Type stubs (.pyi) do not appear; the adapter reads one distribution per root.",
    "A write to an attribute through another reference, such as a parameter or the class's name in another module, or through setattr, stays unseen, so the attribute may keep a type that such a write breaks.",
    "The resolver ignores the order of statements: a name that del removes counts as unknown everywhere.",
]

#: Method names of built-in types; a call of one never gets a heuristic edge.
BUILTIN_METHODS = set()
for _t in (str, bytes, list, dict, set, frozenset, tuple, int, float, object, type, io.TextIOWrapper, io.BytesIO):
    BUILTIN_METHODS.update(dir(_t))
BUILTIN_METHODS.update({"get", "set", "run", "close", "open", "start", "stop", "send", "write", "read", "join", "items", "keys", "values", "update"})


def extract(request: dict) -> dict:
    """Reads the project that a protocol-1 request names and returns its document."""
    root = os.path.abspath(request["root"])
    repo_root = os.path.abspath(request.get("repoRoot") or root)
    options = request.get("options") or {}
    project = build(repo_root, root, request.get("include") or [], request.get("exclude") or [], request.get("tests") or [], options)
    r = Resolver(project)
    out = Output(project, r)
    out.classify()
    out.containers()
    out.symbols()
    out.edges()
    out.tests()
    wanted = [e.get("plugin", "") for e in request.get("entryPoints") or []]
    entry_points = plugins.run(project, r, wanted, {e.get("plugin", ""): e.get("options") or {} for e in request.get("entryPoints") or []})
    return out.document(entry_points)


class Output:
    """Collects the document's lists while phase two runs."""

    def __init__(self, project: Project, r: Resolver) -> None:
        """Prepares empty lists for one project."""
        #: The project that phase one indexed.
        self.p = project
        #: The resolver that gives names their meaning.
        self.r = r
        #: The containers by ID.
        self.cont: dict[str, dict] = {}
        #: The symbol objects, in the order the modules list them.
        self.syms: list[dict] = []
        #: Each edge's first line, count and confidence, by its ends and kind.
        self.edge_map: dict[tuple[str, str, str], list] = {}
        #: The tests that use each symbol, by the symbol's ID.
        self.test_map: dict[str, list[dict]] = {}
        #: The outside packages that each unit imports.
        self.external: dict[str, set[str]] = {}
        #: How many call sites resolved, and how.
        self.stats = {"map": 0, "outside": 0, "unknown": 0, "heuristic": 0}

    # Kinds.

    def classify(self) -> None:
        """Sets each symbol's final kind, word, modifiers and exported flag."""
        syms = [s for m in self.p.modules.values() for s in m.symbols]
        classes = [s for s in syms if s.kind == "class"]
        self._share_writes(classes)
        for _ in range(3):
            for c in classes:
                self._classify_class(c)
        for s in syms:
            if s.parent is None and s.kind == "variable":
                self._classify_variable(s)
            if s.parent is not None and s.parent.kind == "enum" and s.kind == "field" and s.lkind == "class attribute" and s.value is not None:
                s.kind, s.lkind = "constant", "enum member"
            if s.parent is not None and s.parent.lkind in {"dataclass", "typed dict", "named tuple"} and s.lkind == "class attribute":
                s.lkind = "field"
            s.exported = self._exported(s)
        for c in classes:
            if any("abstract" in m.modifiers for m in c.members.values()):
                c.modifiers.add("abstract")

    def _share_writes(self, classes: list[Sym]) -> None:
        """Lets a class and its bases see each other's writes of ``self.x``.

        ``self`` in a base class's method may hold a subclass, and a subclass
        reads what its bases' methods write. So a field takes the writes that
        every class along the method resolution order makes to the same name.
        """
        r = self.r
        for c in classes:
            for k in r.mro(c)[1:]:
                for name, writes in c.self_writes.items():
                    field_sym = k.members.get(name)
                    if field_sym is not None and field_sym.kind == "field":
                        field_sym.writes.extend(writes)
                for name, writes in k.self_writes.items():
                    field_sym = c.members.get(name)
                    if field_sym is not None and field_sym.kind == "field":
                        field_sym.writes.extend(writes)
        # The bases' lookups above may have typed a variable before every
        # write arrived; start the cache afresh.
        r._var.clear()

    def _classify_class(self, c: Sym) -> None:
        """Picks a class's kind and word from its bases and decorators."""
        r = self.r
        outside = {typing_name(("ext", b)) or b for b in r.outside_bases(c)}
        bases = r.project_bases(c)
        decos = {self._deco_name(d, c.module) for d in c.decorators}
        if outside & {"Enum", "enum.Enum", "IntEnum", "enum.IntEnum", "StrEnum", "enum.StrEnum", "Flag", "enum.Flag", "IntFlag", "enum.IntFlag"} or any(b.kind == "enum" for b in bases):
            c.kind, c.lkind = "enum", "enum"
        elif "Protocol" in outside:
            c.kind, c.lkind = "interface", "protocol"
        elif "TypedDict" in outside or any(b.lkind == "typed dict" for b in bases):
            c.kind, c.lkind = "struct", "typed dict"
        elif "NamedTuple" in outside:
            c.kind, c.lkind = "struct", "named tuple"
        elif decos & {"dataclass", "dataclasses.dataclass"}:
            c.kind, c.lkind = "class", "dataclass"
        elif outside & BUILTIN_EXCEPTIONS or any(b.lkind == "exception" for b in bases):
            c.kind, c.lkind = "class", "exception"

    def _deco_name(self, d: ast.expr, mod: Module) -> str:
        """Returns the dotted name that a decorator in ``mod`` resolves to."""
        target = d.func if isinstance(d, ast.Call) else d
        v = self.r.value(target, self.r.ctx_of_module(mod))
        if v and v[0] == "ext":
            return v[1]
        if v and v[0] in {"func", "class"}:
            return v[1].dotted
        return ""

    def _classify_variable(self, s: Sym) -> None:
        """Picks a module-level name's kind: constant, alias, new type or variable."""
        r = self.r
        ctx = r.ctx_of_module(s.module)
        if s.ann is not None:
            head = s.ann.value if isinstance(s.ann, ast.Subscript) else s.ann
            name = typing_name(r.value(head, ctx))
            if name == "TypeAlias":
                s.kind, s.lkind = "type_alias", "type alias"
                return
            if name == "Final":
                s.kind, s.lkind = "constant", "constant"
                return
        if isinstance(s.value, ast.Call):
            name = typing_name(r.value(s.value.func, ctx))
            if name == "NewType":
                s.kind, s.lkind = "type", "new type"
                return
            if name in {"TypeVar", "ParamSpec", "TypeVarTuple"}:
                s.lkind = "type variable"
                return
        if s.name.isupper() or (s.name.strip("_").isupper() and not s.name.startswith("__")):
            s.kind, s.lkind = "constant", "constant"

    def _exported(self, s: Sym) -> bool:
        """Reports whether code outside the unit may use a symbol."""
        if s.lkind == "main guard":
            return False
        if s.parent is None:
            if s.module.all_names is not None:
                return s.name in s.module.all_names
            return not s.name.startswith("_")
        # A member follows its owner: a member of a class that the module
        # does not export stays unexported too.
        return s.parent.exported and not (s.name.startswith("_") and not s.name.endswith("__"))

    # Containers.

    def containers(self) -> None:
        """Writes the distribution, one unit per package, and one file per module."""
        p = self.p
        dist_id = "py:dist:" + p.distribution
        root_rel = p.rel_to_repo(p.root)
        self.cont[dist_id] = {"id": dist_id, "kind": "package", "languageKind": "distribution", "name": p.distribution, "path": root_rel, "language": "py"}
        for mod in p.mapped():
            pkg = mod.name if mod.is_package else mod.name.rpartition(".")[0]
            folder = os.path.dirname(mod.rel)
            if pkg:
                unit_id = "py:pkg:" + pkg
                if unit_id not in self.cont:
                    has_init = os.path.isfile(os.path.join(p.root, folder, "__init__.py"))
                    self.cont[unit_id] = {
                        "id": unit_id,
                        "kind": "package",
                        "languageKind": "package" if has_init else "namespace package",
                        "name": pkg,
                        "path": p.rel_to_repo(os.path.join(p.root, folder)),
                        "parent": dist_id,
                        "language": "py",
                        "unit": True,
                    }
            else:
                path = p.rel_to_repo(os.path.join(p.root, folder))
                unit_id = "py:dir:" + path
                if unit_id not in self.cont:
                    self.cont[unit_id] = {
                        "id": unit_id,
                        "kind": "directory",
                        "languageKind": "folder",
                        "name": folder or os.path.basename(p.root),
                        "path": path,
                        "parent": dist_id,
                        "language": "py",
                        "unit": True,
                    }
            mod.unit_id = unit_id
            mod.file_id = "py:file:" + mod.path
            entry = {"id": mod.file_id, "kind": "file", "languageKind": "module", "name": os.path.basename(mod.rel), "path": mod.path, "parent": unit_id, "language": "py"}
            raw = _module_doc(mod)
            if raw:
                self._current = mod
                doc = docparse.parse(raw, self.doc_resolver(mod, None))
                entry["doc"] = doc
                if mod.is_package:
                    self.cont[unit_id]["doc"] = doc
            self.cont[mod.file_id] = entry

    # Symbols.

    def doc_resolver(self, mod: Module, owner: Optional[Sym]):
        """Returns a function that resolves names in a doc of ``mod``."""
        r = self.r
        p = self.p

        def resolve(target: str) -> Optional[str]:
            """Resolves one name that a doc mentions to an ID of the map."""
            target = target.strip()
            if not target or " " in target:
                return None
            parts = target.split(".")
            if owner is not None and len(parts) == 1:
                cls = owner if owner.kind in TYPE_KINDS else owner.parent
                if cls is not None and cls.kind in TYPE_KINDS:
                    found = r.lookup_mro(cls, target)
                    if found is not None:
                        return self.id_of(("func", found) if found.callable else ("class", found) if found.kind in TYPE_KINDS else ("var", found))
            ctx = r.ctx_of_module(mod)
            sym = r.member_symbol(mod.name, parts[0])
            v = ("var", sym) if sym is not None and len(parts) == 1 else ctx.lookup(parts[0])
            if v is None or v[0] == "ext":
                for i in range(len(parts), 0, -1):
                    name = ".".join(parts[:i])
                    if name in p.modules:
                        v = ("module", name)
                        parts = [name] + parts[i:]
                        break
            if v is None or v[0] == "ext":
                return None
            for part in parts[1:]:
                if v is not None and v[0] in {"class", "inst"}:
                    m = r.lookup_mro(v[1], part)
                    v = None if m is None else (("func", m) if m.callable else ("class", m) if m.kind in TYPE_KINDS else ("var", m))
                elif v is not None and v[0] == "module" and r.member_symbol(v[1], part) is not None:
                    m = r.member_symbol(v[1], part)
                    v = ("var", m)
                else:
                    v = r.attribute(v, part)
            return self.id_of(v)

        return resolve

    def id_of(self, v) -> Optional[str]:
        """Returns the ID of the map that a resolved value names, if any."""
        if v is None:
            return None
        if v[0] == "module":
            mod = self.p.modules.get(v[1])
            if mod is None or mod.is_test:
                return None
            return mod.unit_id if mod.is_package else mod.file_id
        if v[0] in {"func", "class", "var"} and not v[1].module.is_test:
            return v[1].id
        return None

    def symbols(self) -> None:
        """Writes one symbol object per declaration of a mapped module."""
        r = self.r
        for mod in self.p.mapped():
            self._current = mod
            for s in mod.symbols:
                out: dict = {
                    "id": s.id,
                    "kind": s.kind,
                    "languageKind": s.lkind,
                    "name": s.name,
                    "parent": s.parent.id if s.parent else mod.file_id,
                    "language": "py",
                    "exported": s.exported,
                    "location": {"path": mod.path, "line": s.line, "endLine": s.end_line},
                }
                if s.doc_raw:
                    out["doc"] = docparse.parse(s.doc_raw, self.doc_resolver(mod, s))
                tp = getattr(s.node, "type_params", None)
                if tp:
                    out["typeParams"] = "[" + ", ".join(mod.segment(t) for t in tp) + "]"
                if s.kind in {"function", "method", "constructor"} and s.lkind != "main guard":
                    out["signature"] = self.signature(s)
                if s.kind == "property":
                    ret = getattr(s.node, "returns", None)
                    if ret is not None:
                        out["type"] = {"text": mod.segment(ret), "refs": r.refs(ret, mod)}
                elif s.kind in {"field", "constant", "variable"} and s.ann is not None:
                    out["type"] = {"text": (s.module.segment(s.ann)), "refs": r.refs(s.ann, s.module)}
                elif s.kind == "field" and isinstance(s.value, ast.Call) and s.method is not None and len(s.writes) == 1:
                    v = r.value(s.value.func, r.ctx_of(s.method, mod, s.parent))
                    if v is not None and v[0] == "class" and not v[1].module.is_test:
                        out["type"] = {"text": mod.segment(s.value.func), "refs": [v[1].id]}
                elif s.kind == "type_alias" and s.value is not None:
                    out["type"] = {"text": mod.segment(s.value), "refs": r.refs(s.value, mod)}
                elif s.kind == "type" and isinstance(s.value, ast.Call) and len(s.value.args) > 1:
                    out["type"] = {"text": mod.segment(s.value.args[1]), "refs": r.refs(s.value.args[1], mod)}
                if "type" in out and not out["type"]["refs"]:
                    del out["type"]["refs"]
                if s.kind == "constant" and s.value is not None and s.lkind != "enum member" or s.lkind == "enum member" and s.value is not None:
                    text = mod.segment(s.value)
                    out["value"] = text if len(text) <= 80 else text[:79] + "…"
                if s.modifiers:
                    out["modifiers"] = sorted(s.modifiers)
                if s.decorators:
                    out["annotations"] = ["@" + mod.segment(d) for d in s.decorators]
                sat = self._satisfies(s)
                if sat:
                    out["satisfies"] = sat
                self.syms.append(out)

    def _satisfies(self, s: Sym) -> list[str]:
        """Lists the outside bases of a class, such as ``Exception`` or ``ABC``."""
        if s.kind not in TYPE_KINDS or s.kind in {"type_alias", "type"}:
            return []
        out = []
        for b in self.r.outside_bases(s):
            name = b
            for prefix in ("builtins.", "typing.", "typing_extensions."):
                if name.startswith(prefix):
                    name = name[len(prefix) :]
            if name not in {"object", "Protocol", "Generic", "TypedDict", "NamedTuple"}:
                out.append(name)
        return sorted(set(out))

    def signature(self, s: Sym) -> dict:
        """Builds a callable's signature from its ``def`` line."""
        r = self.r
        mod = s.module
        fn = s.node
        a = fn.args  # type: ignore[attr-defined]
        positional = a.posonlyargs + a.args
        defaults = [None] * (len(positional) - len(a.defaults)) + list(a.defaults)
        params = []
        texts = []

        def one(arg: ast.arg, default: Optional[ast.expr], rest: str = "") -> None:
            """Adds one parameter to the signature and to its text."""
            text = rest + arg.arg
            p: dict = {"name": arg.arg, "type": mod.segment(arg.annotation) if arg.annotation is not None else ""}
            if arg.annotation is not None:
                text += ": " + mod.segment(arg.annotation)
                refs = r.refs(arg.annotation, mod)
                if refs:
                    p["refs"] = refs
            if default is not None:
                p["optional"] = True
                p["default"] = mod.segment(default)
                text += (" = " if arg.annotation is not None else "=") + mod.segment(default)
            if rest:
                p["rest"] = True
            params.append(p)
            texts.append(text)

        skip = 1 if s.parent is not None and s.kind in {"method", "constructor"} and "staticmethod" not in s.modifiers and positional else 0
        for i, arg in enumerate(positional):
            if i < skip:
                texts.append(arg.arg)
                continue
            one(arg, defaults[i])
            if a.posonlyargs and i == len(a.posonlyargs) - 1:
                texts.append("/")
        if a.vararg is not None:
            one(a.vararg, None, "*")
        elif a.kwonlyargs:
            texts.append("*")
        for arg, d in zip(a.kwonlyargs, a.kw_defaults):
            one(arg, d)
        if a.kwarg is not None:
            one(a.kwarg, None, "**")
        results = []
        text = ("async def " if "async" in s.modifiers else "def ") + s.name
        tp = getattr(fn, "type_params", None)
        if tp:
            text += "[" + ", ".join(mod.segment(t) for t in tp) + "]"
        text += "(" + ", ".join(texts) + ")"
        if fn.returns is not None:  # type: ignore[attr-defined]
            res: dict = {"type": mod.segment(fn.returns)}  # type: ignore[attr-defined]
            refs = r.refs(fn.returns, mod)  # type: ignore[attr-defined]
            if refs:
                res["refs"] = refs
            results.append(res)
            text += " -> " + mod.segment(fn.returns)  # type: ignore[attr-defined]
        return {"params": params, "results": results, "text": text}

    # Edges.

    def edge(self, frm: str, to: str, kind: str, line: Optional[int], confidence: str = "exact") -> None:
        """Records one use; repeats raise its count and keep the first line."""
        if frm == to and kind != "calls":
            return
        key = (frm, to, kind)
        if key in self.edge_map:
            e = self.edge_map[key]
            e[1] += 1
            if line is not None and (e[0] is None or line < e[0]):
                e[0] = line
            if confidence == "exact":
                e[2] = "exact"
        else:
            self.edge_map[key] = [line, 1, confidence]

    def edges(self) -> None:
        """Resolves calls, type uses, class relations and imports."""
        r = self.r
        #: The methods whose names only one mapped class defines.
        self.unique = self._unique_methods()
        for mod in self.p.mapped():
            self._current = mod
            for s in mod.symbols:
                if s.kind in TYPE_KINDS and s.kind not in {"type_alias", "type"}:
                    self._class_edges(s)
                if s.callable or s.lkind == "main guard":
                    self._calls(s)
                if s.kind in {"function", "method", "constructor"} and s.lkind != "main guard":
                    fn = s.node
                    for arg in fn.args.posonlyargs + fn.args.args + fn.args.kwonlyargs + [fn.args.vararg, fn.args.kwarg]:  # type: ignore[attr-defined]
                        if arg is not None and arg.annotation is not None:
                            for ref in r.refs(arg.annotation, mod):
                                self.edge(s.id, ref, "param_type", arg.annotation.lineno)
                    if fn.returns is not None:  # type: ignore[attr-defined]
                        for ref in r.refs(fn.returns, mod):  # type: ignore[attr-defined]
                            self.edge(s.id, ref, "return_type", fn.returns.lineno)  # type: ignore[attr-defined]
                if s.kind == "property" and getattr(s.node, "returns", None) is not None:
                    for ref in r.refs(s.node.returns, mod):  # type: ignore[attr-defined]
                        self.edge(s.id, ref, "field_type", s.node.returns.lineno)  # type: ignore[attr-defined]
                if s.kind in {"field", "constant", "variable"} and s.ann is not None:
                    for ref in r.refs(s.ann, s.module):
                        self.edge(s.id, ref, "field_type", s.line)
            self._imports(mod)

    def _unique_methods(self) -> dict[str, Sym]:
        """Maps each method name that exactly one mapped class defines to its method."""
        seen: dict[str, list[Sym]] = {}
        for mod in self.p.mapped():
            for s in mod.symbols:
                if s.kind in {"method"} and s.parent is not None:
                    seen.setdefault(s.name, []).append(s)
        return {n: v[0] for n, v in seen.items() if len(v) == 1 and not n.startswith("__") and n not in BUILTIN_METHODS}

    def _class_edges(self, c: Sym) -> None:
        """Adds ``extends``, ``implements`` and ``overrides`` for one class."""
        r = self.r
        for b in r.project_bases(c):
            if b.module.is_test:
                continue
            kind = "implements" if b.kind == "interface" and c.kind != "interface" else "extends"
            self.edge(c.id, b.id, kind, c.line)
        mro = r.mro(c)
        for m in c.members.values():
            if m.kind not in {"method", "property"} or m.parent is not c:
                continue
            for k in mro[1:]:
                other = k.members.get(m.name)
                if other is not None and other.kind in {"method", "property"} and not k.module.is_test:
                    self.edge(m.id, other.id, "overrides", m.line)
                    break

    def _calls(self, s: Sym) -> None:
        """Adds the ``calls`` edges of one callable symbol."""
        r = self.r
        if s.lkind == "main guard":
            ctx = r.ctx_of_module(s.module)
            self._walk_calls(s, s.node.body, ctx)  # type: ignore[attr-defined]
        else:
            cls = s.parent if s.parent is not None and s.parent.kind in TYPE_KINDS else None
            for node in [s.node] + s.extra:
                ctx = r.ctx_of(node, s.module, cls)
                self._walk_calls(s, node.body, ctx)  # type: ignore[attr-defined]
            mctx = r.ctx_of_module(s.module)
            for d in s.decorators:
                target = d.func if isinstance(d, ast.Call) else d
                v = r.value(target, mctx)
                if v is not None and v[0] == "func" and v[1].kind in {"function", "method"} and not v[1].module.is_test:
                    self.edge(s.id, v[1].id, "calls", d.lineno)

    def _walk_calls(self, s: Sym, body: list, ctx: Ctx) -> None:
        """Walks statements for calls, giving nested functions their own scope."""
        r = self.r
        stack: list[tuple[ast.AST, Ctx]] = [(n, ctx) for n in body]
        while stack:
            node, c = stack.pop()
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)):
                inner = r.ctx_of(node, s.module, None, c)
                for d in getattr(node, "decorator_list", []):
                    stack.append((d, c))
                children = node.body if isinstance(node.body, list) else [node.body]
                stack.extend((n, inner) for n in children)
                continue
            if isinstance(node, ast.ClassDef):
                stack.extend((n, c) for n in node.body)
                continue
            if isinstance(node, ast.Call):
                self._one_call(s, node, c)
            stack.extend((n, c) for n in ast.iter_child_nodes(node))

    def _one_call(self, s: Sym, call: ast.Call, ctx: Ctx) -> None:
        """Resolves one call site and records its edge."""
        r = self.r
        v = r.value(call.func, ctx)
        target = None
        if v is not None and v[0] == "func" and v[1].kind in {"function", "method", "constructor"}:
            target = v[1]
        elif v is not None and v[0] == "class":
            cls = r.alias_target(v[1])
            init = r.lookup_mro(cls, "__init__") if cls is not None else None
            if init is not None and init.kind == "constructor":
                target = init
            else:
                self.stats["map"] += 1
                return
        if target is not None:
            if target.module.is_test:
                return
            self.stats["map"] += 1
            self.edge(s.id, target.id, "calls", call.lineno)
            return
        if v is not None:
            self.stats["outside"] += 1
            return
        if isinstance(call.func, ast.Attribute):
            base = r.value(call.func.value, ctx)
            guess = self.unique.get(call.func.attr)
            if base is None and guess is not None:
                self.stats["heuristic"] += 1
                self.edge(s.id, guess.id, "calls", call.lineno, "heuristic")
                return
        self.stats["unknown"] += 1

    def _imports(self, mod: Module) -> None:
        """Adds a module's imports as edges between units, and its outside packages."""
        p = self.p
        tops = {m.split(".")[0] for m in p.modules}
        for node in ast.walk(mod.tree):
            names: list[str] = []
            if isinstance(node, ast.Import):
                names = [a.name for a in node.names]
            elif isinstance(node, ast.ImportFrom):
                base = resolve_relative(mod, node.level, node.module)
                if base is None:
                    continue
                names = [base + "." + a.name if base + "." + a.name in p.modules else base for a in node.names]
            for name in names:
                target = None
                parts = name.split(".")
                for i in range(len(parts), 0, -1):
                    cand = ".".join(parts[:i])
                    if cand in p.modules:
                        target = p.modules[cand]
                        break
                if target is not None:
                    if not target.is_test and target.unit_id and target.unit_id != mod.unit_id:
                        self.edge(mod.unit_id, target.unit_id, "imports", None)
                    continue
                top = parts[0]
                if top and top not in STDLIB and top not in tops and not hasattr(builtins, top):
                    self.external.setdefault(mod.unit_id, set()).add(top)

    # Tests.

    def tests(self) -> None:
        """Finds the tests and records which symbols each one uses."""
        r = self.r
        for mod in sorted((m for m in self.p.modules.values() if m.is_test), key=lambda m: m.rel):
            for name, fn, cls in _test_functions(mod, r):
                ctx = r.ctx_of(fn, mod, cls)
                used: set[str] = set()
                for node in ast.walk(fn):
                    if isinstance(node, (ast.Name, ast.Attribute)) and isinstance(node.ctx, ast.Load):
                        v = r.value(node, ctx)
                        if v is not None and v[0] in {"func", "class"} and not v[1].module.is_test:
                            used.add(v[1].id)
                for sym_id in used:
                    self.test_map.setdefault(sym_id, []).append({"name": name, "location": {"path": mod.path, "line": fn.lineno}})

    # Document.

    def document(self, entry_points: list[dict]) -> dict:
        """Assembles the sorted document."""
        for unit, names in self.external.items():
            if unit in self.cont:
                self.cont[unit]["externalImports"] = sorted(names)
        edges = []
        for (frm, to, kind), (line, count, conf) in self.edge_map.items():
            e = {"from": frm, "to": to, "kind": kind, "confidence": conf}
            if line is not None:
                e["line"] = line
            if count > 1:
                e["count"] = count
            edges.append(e)
        edges.sort(key=lambda e: (e["from"], e["kind"], e.get("line", 0), e["to"]))
        notes = list(LIMITS)
        total = sum(self.stats.values())
        if total:
            notes.append(
                f"Of {total} call sites, {self.stats['map']} resolved to code on the map, {self.stats['outside']} to outside code or built-ins, "
                f"{self.stats['heuristic']} by a unique method name (heuristic), and {self.stats['unknown']} to nothing."
            )
        ext = sum(len(v) for v in self.external.values())
        if ext:
            notes.append(f"Units import {ext} outside packages; the adapter installs nothing, so their types count as unknown.")
        if self.p.skipped:
            notes.append("These files stay out of the map: " + ", ".join(sorted(self.p.skipped)) + ".")
        doc: dict = {
            "version": 1,
            "meta": {
                "schemaVersion": 1,
                "tool": {"name": NAME, "version": VERSION},
                "generated": "",
                "languages": ["py"],
                "adapters": [{"name": "python", "version": VERSION, "language": "py", "root": self.p.rel_to_repo(self.p.root), "notes": notes}],
            },
            "containers": sorted(self.cont.values(), key=lambda c: c["id"]),
            "symbols": sorted(self.syms, key=lambda s: s["id"]),
            "edges": edges,
        }
        if entry_points:
            doc["entryPoints"] = entry_points
        if self.test_map:
            doc["tests"] = {k: sorted(v, key=lambda t: (t["location"]["path"], t["location"]["line"], t["name"])) for k, v in sorted(self.test_map.items())}
        return doc


def _module_doc(mod: Module) -> Optional[str]:
    """Returns a module's docstring."""
    from .model import docstring

    return docstring(mod.tree)


def _test_functions(mod: Module, r: Resolver):
    """Yields each test of a test module: its pytest name, function and class."""
    for stmt in mod.tree.body:
        if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef)) and stmt.name.startswith("test"):
            yield mod.rel + "::" + stmt.name, stmt, None
        elif isinstance(stmt, ast.ClassDef):
            cls = next((s for s in mod.symbols if s.node is stmt), None)
            unittest_case = False
            if cls is not None:
                for k in r.mro(cls):
                    if any(b.startswith("unittest.") and b.endswith("TestCase") for b in r.outside_bases(k)):
                        unittest_case = True
            if not (stmt.name.startswith("Test") or unittest_case):
                continue
            for item in stmt.body:
                if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)) and item.name.startswith("test"):
                    yield mod.rel + "::" + stmt.name + "::" + item.name, item, cls
