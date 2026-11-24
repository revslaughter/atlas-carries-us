"""Finds entry points: HTTP routes, commands and main guards.

Each plugin reads the syntax trees that phase one indexed. A receiver, such
as a Flask app or a click group, counts only when the resolver traces it to
the framework: an outside import, or the framework's own class when the
project holds the framework itself.
"""

from __future__ import annotations

import ast
from dataclasses import dataclass, field
from typing import Iterator, Optional

from .index import Project
from .model import Module, Sym
from .resolve import Ctx, Resolver

#: The plugins that this adapter offers.
KNOWN = ["click", "django", "fastapi", "flask", "python-main", "typer"]

#: HTTP verbs that a decorator such as ``@app.get`` names.
VERBS = ["get", "post", "put", "patch", "delete", "options", "head", "trace"]

#: The dotted names of Flask's application class.
_FLASK_APP = {"flask.Flask", "flask.app.Flask"}
#: The dotted names of Flask's blueprint class.
_FLASK_BP = {"flask.Blueprint", "flask.blueprints.Blueprint"}
#: The dotted names of FastAPI's application class.
_FASTAPI_APP = {"fastapi.FastAPI", "fastapi.applications.FastAPI"}
#: The dotted names of FastAPI's router class.
_FASTAPI_ROUTER = {"fastapi.APIRouter", "fastapi.routing.APIRouter"}
#: The dotted names of click's group classes.
_CLICK_GROUP = {"click.Group", "click.core.Group", "click.MultiCommand", "click.core.MultiCommand", "click.CommandCollection"}
#: Maps click's decorators to the kind of command they make.
_CLICK_DECORATORS = {"click.command": "command", "click.group": "group", "click.decorators.command": "command", "click.decorators.group": "group"}
#: The dotted names of Typer's application class.
_TYPER = {"typer.Typer", "typer.main.Typer"}
#: The dotted names of Django's URL pattern functions.
_DJANGO_PATH = {"django.urls.path", "django.urls.re_path", "django.urls.conf.path", "django.urls.conf.re_path", "django.conf.urls.url"}
#: The dotted names of Django's ``include``.
_DJANGO_INCLUDE = {"django.urls.include", "django.urls.conf.include", "django.conf.urls.include"}


class UnknownPlugin(Exception):
    """Reports a plugin name that this adapter does not offer."""


@dataclass
class Obj:
    """Holds one framework object: an app, a router, a blueprint or a group."""

    #: What the object is: app, blueprint, router, group or command.
    kind: str
    #: The command or app name.
    name: str = ""
    #: The URL prefix that the object declares itself.
    prefix: str = ""
    #: The module that builds or declares the object.
    module: Optional[Module] = None
    #: The line that builds or declares it.
    line: int = 0
    #: Each parent that mounts the object, with the mount's prefix.
    mounts: list[tuple[object, str]] = field(default_factory=list)
    #: Each command or app below the object, with a name the mount gives.
    children: list[tuple[object, Optional[str]]] = field(default_factory=list)
    #: The ID of the symbol that serves the object.
    handler: Optional[str] = None
    #: True when the handler serves inline, inside another function.
    inline: bool = False
    #: The project class that builds the object, if any.
    cls: Optional[Sym] = None


@dataclass
class Site:
    """Holds one node of a module with the scope and the symbol around it."""

    #: The syntax node.
    node: ast.AST
    #: The scope around the node.
    ctx: Ctx
    #: The function or method around the node.
    owner: Optional[Sym]
    #: The module that holds the node.
    module: Module


class Finder:
    """Shares the walks and lookups that every plugin uses."""

    def __init__(self, project: Project, r: Resolver) -> None:
        """Indexes the symbols of the mapped modules by their syntax node."""
        #: The project that the plugins read.
        self.p = project
        #: The resolver that gives names their meaning.
        self.r = r
        #: The mapped symbols by the identity of their syntax node.
        self.by_node: dict[int, Sym] = {}
        for mod in project.mapped():
            for s in mod.symbols:
                self.by_node[id(s.node)] = s
        #: The entry points found so far.
        self.entries: list[dict] = []

    def sites(self) -> Iterator[Site]:
        """Yields every node of every mapped module, with its scope and owner."""
        r = self.r
        for mod in self.p.mapped():
            mctx = r.ctx_of_module(mod)
            stack: list[tuple[ast.AST, Ctx, Optional[Sym], Optional[Sym]]] = [(n, mctx, None, None) for n in reversed(mod.tree.body)]
            while stack:
                node, ctx, owner, cls = stack.pop()
                yield Site(node, ctx, owner, mod)
                if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    sym = self.by_node.get(id(node))
                    inner = r.ctx_of(node, mod, cls, ctx if ctx.func is not None else None)
                    new_owner = owner or (sym if sym is not None and sym.callable else None)
                    stack.extend((n, inner, new_owner, None) for n in reversed(node.body))
                    stack.extend((d, ctx, owner, cls) for d in node.decorator_list)
                elif isinstance(node, ast.ClassDef):
                    sym = self.by_node.get(id(node))
                    stack.extend((n, ctx, owner, sym) for n in reversed(node.body))
                elif isinstance(node, ast.If) and mod.guard is not None and node is mod.guard.node:
                    stack.extend((n, ctx, mod.guard, cls) for n in reversed(node.body))
                else:
                    stack.extend((n, ctx, owner, cls) for n in reversed(list(ast.iter_child_nodes(node))))

    def key(self, expr: ast.expr, ctx: Ctx) -> Optional[tuple]:
        """Names the binding that an expression reads, so that objects keep their identity."""
        if isinstance(expr, ast.Name):
            c: Optional[Ctx] = ctx
            while c is not None and c.func is not None:
                if expr.id in c.locals:
                    return ("local", id(c.func), expr.id)
                c = c.parent
            return self.module_key(ctx.module.name, expr.id)
        if isinstance(expr, ast.Attribute):
            base = self.r.value(expr.value, ctx)
            if base is not None and base[0] == "module":
                return self.module_key(base[1], expr.attr)
        return None

    def module_key(self, modname: str, name: str, depth: int = 0) -> Optional[tuple]:
        """Follows imports to the symbol that a module-level name binds."""
        mod = self.p.modules.get(modname)
        if mod is None or depth > 20:
            return None
        b = mod.scope.get(name)
        if b is None:
            for star in mod.star_imports:
                found = self.module_key(star, name, depth + 1)
                if found:
                    return found
            return None
        if b.kind == "symbol" and b.symbol is not None:
            return ("sym", b.symbol.id)
        if b.kind == "from":
            return self.module_key(b.target, b.attr or "", depth + 1)
        return None

    def handler(self, fn: ast.AST, owner: Optional[Sym]) -> tuple[Optional[str], bool]:
        """Returns the handler of a decorated function, and whether it serves inline."""
        sym = self.by_node.get(id(fn))
        if sym is not None and owner is None:
            return sym.id, False
        if owner is not None:
            return owner.id, True
        return None, False

    def add(self, kind: str, name: str, handler: str, mod: Module, line: int, plugin: str, method: str = "", path: str = "", inline: bool = False, registrar: Optional[Sym] = None, at: Optional[dict] = None) -> None:
        """Records one entry point."""
        e: dict = {"id": kind + ":" + name, "kind": kind, "name": name}
        if method:
            e["method"] = method
        if path:
            e["path"] = path
        e["handler"] = handler
        if inline:
            e["inline"] = True
        if registrar is not None and registrar.id != handler:
            e["registrar"] = registrar.id
        e["registeredAt"] = at or {"path": mod.path, "line": line}
        e["plugin"] = plugin
        self.entries.append(e)

    def constant(self, node: Optional[ast.expr]) -> Optional[str]:
        """Returns a string constant's value, else None."""
        if isinstance(node, ast.Constant) and isinstance(node.value, str):
            return node.value
        return None

    def kwarg(self, call: ast.Call, name: str) -> Optional[ast.expr]:
        """Returns the value of one keyword argument of a call."""
        return next((k.value for k in call.keywords if k.arg == name), None)

    def strings(self, node: Optional[ast.expr]) -> list[str]:
        """Returns the strings of a list or tuple of constants."""
        if isinstance(node, (ast.List, ast.Tuple, ast.Set)):
            return [e.value for e in node.elts if isinstance(e, ast.Constant) and isinstance(e.value, str)]
        return []

    def script_names(self) -> dict[tuple, str]:
        """Maps the binding that each ``[project.scripts]`` entry names to the script's name."""
        out: dict[tuple, str] = {}
        for script, (target, _line) in sorted(self.p.scripts.items()):
            modname, _, attr = target.partition(":")
            key = self.module_key(modname.strip(), attr.strip().split(".")[0])
            if key is not None:
                out.setdefault(key, script)
        return out


def run(project: Project, r: Resolver, wanted: list[str], options: dict) -> list[dict]:
    """Runs the requested plugins and returns their sorted entry points."""
    for name in wanted:
        if name not in KNOWN:
            raise UnknownPlugin(f"the entry-point plugin {name!r} does not exist; use one of {', '.join(KNOWN)}")
    f = Finder(project, r)
    sites = list(f.sites())
    for name in ["flask", "fastapi", "django", "click", "typer", "python-main"]:
        if name in wanted:
            {"flask": flask, "fastapi": fastapi, "django": django, "click": click, "typer": typer, "python-main": python_main}[name](f, sites)
    seen: set[str] = set()
    out = []
    for e in sorted(f.entries, key=lambda e: (e["id"], e["registeredAt"]["path"], e["registeredAt"]["line"])):
        if e["id"] in seen:
            e["id"] = e["id"] + "@" + e["registeredAt"]["path"] + ":" + str(e["registeredAt"]["line"])
        seen.add(e["id"])
        out.append(e)
    return sorted(out, key=lambda e: e["id"])


# HTTP routes.


def _objects(f: Finder, sites: list[Site], kinds: dict[str, set[str]], prefix_kw: str) -> dict[tuple, Obj]:
    """Finds assignments that build one of the named framework classes."""
    out: dict[tuple, Obj] = {}
    for s in sites:
        n = s.node
        if not (isinstance(n, (ast.Assign, ast.AnnAssign)) and isinstance(n.value, ast.Call)):
            continue
        targets = n.targets if isinstance(n, ast.Assign) else [n.target]
        if len(targets) != 1 or not isinstance(targets[0], ast.Name):
            continue
        names = f.r.class_names(f.r.value(n.value.func, s.ctx))
        for kind, wanted in kinds.items():
            if names & wanted:
                key = f.key(targets[0], s.ctx)
                if key is not None:
                    name = f.constant(f.kwarg(n.value, "name")) or (f.constant(n.value.args[0]) if n.value.args else None) or targets[0].id
                    cls = f.r.value(n.value.func, s.ctx)
                    out[key] = Obj(kind=kind, name=name, prefix=f.constant(f.kwarg(n.value, prefix_kw)) or "", module=s.module, line=n.lineno, cls=cls[1] if cls and cls[0] == "class" else None)
    return out


def _receiver(f: Finder, call: ast.Call, ctx: Ctx, objs: dict[tuple, Obj]) -> tuple[Optional[tuple], str]:
    """Returns the object key and the method name of a call such as ``app.get(…)``."""
    if isinstance(call.func, ast.Attribute):
        key = f.key(call.func.value, ctx)
        if key in objs:
            return key, call.func.attr
    return None, ""


def _flask_join(prefix: str, rule: str) -> str:
    """Joins a blueprint prefix and a rule, as Flask does."""
    if not prefix:
        return rule
    if not rule:
        return prefix
    return prefix.rstrip("/") + "/" + rule.lstrip("/")


def _routes(f: Finder, sites: list[Site], objs: dict[tuple, Obj], plugin: str, join, mount_names: set[str], route_names: set[str], rule_adder: str) -> None:
    """Shared by the Flask and FastAPI plugins: routes, mounts and prefixes."""
    routes: list[tuple[tuple, str, list[str], str, bool, Site, int]] = []
    for s in sites:
        n = s.node
        if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef)):
            for d in n.decorator_list:
                if not isinstance(d, ast.Call):
                    continue
                key, method = _receiver(f, d, s.ctx, objs)
                if key is None or not d.args or f.constant(d.args[0]) is None:
                    continue
                if method in VERBS:
                    methods = [method.upper()]
                elif method in route_names:
                    methods = [m.upper() for m in f.strings(f.kwarg(d, "methods"))] or ["GET"]
                else:
                    continue
                handler, inline = f.handler(n, s.owner)
                if handler is not None:
                    routes.append((key, f.constant(d.args[0]) or "", methods, handler, inline, s, d.lineno))
        elif isinstance(n, ast.Call):
            key, method = _receiver(f, n, s.ctx, objs)
            if key is None:
                continue
            if method in mount_names and n.args:
                child = f.key(n.args[0], s.ctx)
                if child in objs:
                    kw = "url_prefix" if plugin == "flask" else "prefix"
                    given = f.constant(f.kwarg(n, kw))
                    objs[child].mounts.append((key, given if given is not None else (objs[child].prefix if plugin == "flask" else "")))
            elif method == rule_adder:
                rule = f.constant(n.args[0]) if n.args else f.constant(f.kwarg(n, "rule") or f.kwarg(n, "path"))
                view = f.kwarg(n, "view_func") or f.kwarg(n, "endpoint")
                if plugin == "flask" and view is None and len(n.args) > 2:
                    view = n.args[2]
                if plugin == "fastapi" and view is None and len(n.args) > 1:
                    view = n.args[1]
                v = f.r.value(view, s.ctx) if view is not None else None
                if rule is None or v is None or v[0] != "func" or v[1].module.is_test:
                    continue
                methods = [m.upper() for m in f.strings(f.kwarg(n, "methods"))] or ["GET"]
                routes.append((key, rule, methods, v[1].id, False, s, n.lineno))

    def prefixes(key: tuple, depth: int = 0) -> list[str]:
        """Returns every prefix that the mounts above an object add."""
        o = objs[key]
        if depth > 10:
            return [o.prefix]
        if plugin == "flask":
            if not o.mounts:
                return [o.prefix]
            return [_flask_join(pp, p) for parent, p in o.mounts for pp in prefixes(parent, depth + 1)]  # type: ignore[arg-type]
        own = o.prefix
        if not o.mounts:
            return [own]
        return [pp + p + own for parent, p in o.mounts for pp in prefixes(parent, depth + 1)]  # type: ignore[arg-type]

    for key, rule, methods, handler, inline, s, line in routes:
        for prefix in sorted(set(prefixes(key))):
            path = join(prefix, rule)
            for m in methods:
                f.add("http", m + " " + path, handler, s.module, line, plugin, method=m, path=path, inline=inline, registrar=s.owner if inline else None)


def flask(f: Finder, sites: list[Site]) -> None:
    """Finds Flask routes, ``add_url_rule`` calls and blueprints."""
    objs = _objects(f, sites, {"app": _FLASK_APP, "blueprint": _FLASK_BP}, "url_prefix")
    _routes(f, sites, objs, "flask", _flask_join, {"register_blueprint"}, {"route"}, "add_url_rule")


def fastapi(f: Finder, sites: list[Site]) -> None:
    """Finds FastAPI routes, ``api_route`` and ``include_router``."""
    objs = _objects(f, sites, {"app": _FASTAPI_APP, "router": _FASTAPI_ROUTER}, "prefix")
    _routes(f, sites, objs, "fastapi", lambda prefix, rule: prefix + rule, {"include_router"}, {"api_route"}, "add_api_route")


def django(f: Finder, sites: list[Site]) -> None:
    """Finds Django ``urlpatterns``, their ``include`` calls and their views."""
    r = f.r
    patterns: dict[str, tuple[Module, list[ast.expr]]] = {}
    roots: list[str] = []
    for s in sites:
        n = s.node
        if s.owner is not None or s.ctx.func is not None:
            continue
        if isinstance(n, (ast.Assign, ast.AugAssign, ast.AnnAssign)):
            targets = n.targets if isinstance(n, ast.Assign) else [n.target]
            names = [t.id for t in targets if isinstance(t, ast.Name)]
            if "ROOT_URLCONF" in names and f.constant(n.value):
                roots.append(f.constant(n.value) or "")
            if "urlpatterns" in names:
                patterns.setdefault(s.module.name, (s.module, []))[1].extend(_list_items(n.value))
    included: set[str] = set()

    def walk(modname: str, items: list[ast.expr], mod: Module, prefix: str, depth: int) -> None:
        """Reads one list of URL patterns under a prefix."""
        if depth > 10:
            return
        ctx = r.ctx_of_module(mod)
        for item in items:
            if not isinstance(item, ast.Call) or not item.args:
                continue
            fn = r.value(item.func, ctx)
            ok = (fn is not None and fn[0] == "ext" and fn[1] in _DJANGO_PATH) or (fn is None and isinstance(item.func, ast.Name) and item.func.id in {"path", "re_path"})
            route = f.constant(item.args[0])
            if not ok or route is None or len(item.args) < 2:
                continue
            view = item.args[1]
            if isinstance(view, ast.Call) and _is_include(r.value(view.func, ctx)):
                target = view.args[0] if view.args else None
                if isinstance(target, ast.Tuple) and target.elts:
                    target = target.elts[0]
                name = f.constant(target)
                if name is None and target is not None:
                    v = r.value(target, ctx)
                    name = v[1] if v and v[0] == "module" else None
                if name in patterns:
                    included.add(name)
                    walk(name, patterns[name][1], patterns[name][0], prefix + route, depth + 1)
                elif isinstance(target, (ast.List, ast.Tuple)):
                    walk(modname, list(target.elts), mod, prefix + route, depth + 1)
                continue
            path = "/" + prefix + route
            if isinstance(view, ast.Call) and isinstance(view.func, ast.Attribute) and view.func.attr == "as_view":
                v = r.value(view.func.value, ctx)
                if v is None or v[0] != "class":
                    continue
                for m in VERBS:
                    found = r.lookup_mro(v[1], m)
                    if found is not None and found.callable and not found.module.is_test:
                        f.add("http", m.upper() + " " + path, found.id, mod, item.lineno, "django", method=m.upper(), path=path)
                continue
            v = r.value(view, ctx)
            if v is not None and v[0] == "func" and not v[1].module.is_test:
                f.add("http", "ANY " + path, v[1].id, mod, item.lineno, "django", method="ANY", path=path)

    starts = [x for x in roots if x in patterns]
    if not starts:
        for name in sorted(patterns):
            walk(name, patterns[name][1], patterns[name][0], "", 0)
        found = list(f.entries)
        f.entries = [e for e in found if e["plugin"] != "django"]
        included_now = set(included)
        included.clear()
        starts = [n for n in sorted(patterns) if n not in included_now]
    for name in starts:
        walk(name, patterns[name][1], patterns[name][0], "", 0)


def _is_include(v) -> bool:
    """Reports whether a value names Django's ``include``."""
    return v is not None and v[0] == "ext" and v[1] in _DJANGO_INCLUDE


def _list_items(node: Optional[ast.expr]) -> list[ast.expr]:
    """Returns the elements of a list, or of lists joined with ``+``."""
    if isinstance(node, (ast.List, ast.Tuple)):
        return list(node.elts)
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Add):
        return _list_items(node.left) + _list_items(node.right)
    return []


# Commands.


def _command_name(fn_name: str, strip: bool) -> str:
    """Turns a function's name into a command's name, as click and Typer do."""
    name = fn_name.lower().replace("_", "-")
    if strip:
        for suffix in ("-command", "-cmd", "-group", "-grp"):
            if name.endswith(suffix) and name != suffix:
                name = name[: -len(suffix)]
                break
    return name


def _tree_entries(f: Finder, nodes: dict[tuple, Obj], plugin: str, scripts: dict[tuple, str]) -> None:
    """Writes one entry per leaf command, with names joined from the root."""
    parents: dict[tuple, list[tuple[tuple, Optional[str]]]] = {}
    for key, node in nodes.items():
        for child, name in node.children:
            if child in nodes:
                parents.setdefault(child, []).append((key, name))  # type: ignore[arg-type]

    def names(key: tuple, depth: int = 0) -> list[str]:
        """Returns every full name of a command."""
        node = nodes[key]
        if key not in parents or depth > 10:
            return [scripts.get(key, node.name)]
        out = []
        for parent, given in parents[key]:
            for pn in names(parent, depth + 1):
                own = given if given is not None else node.name
                out.append((pn + " " + own).strip() if own else pn)
        return out

    for key, node in nodes.items():
        has_children = any(c in nodes for c, _ in node.children)
        if has_children or node.handler is None or node.module is None:
            continue
        for full in sorted(set(names(key))):
            f.add("cli", full, node.handler, node.module, node.line, plugin, inline=node.inline)


def click(f: Finder, sites: list[Site]) -> None:
    """Finds click commands, groups and ``add_command`` calls."""
    r = f.r
    nodes: dict[tuple, Obj] = {}
    for key, o in _objects(f, sites, {"group": _CLICK_GROUP}, "_").items():
        nodes[key] = o
    class_adds: list[tuple[Sym, ast.expr, Optional[str], Ctx]] = []
    later: list[tuple[tuple, ast.expr, Optional[str], Ctx]] = []
    for s in sites:
        n = s.node
        if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef)):
            for d in n.decorator_list:
                base = d.func if isinstance(d, ast.Call) else d
                v = r.value(base, s.ctx)
                kind = _CLICK_DECORATORS.get(v[1]) if v and v[0] == "ext" else None
                parent = None
                if kind is None and isinstance(base, ast.Attribute) and base.attr in {"command", "group"}:
                    parent = f.key(base.value, s.ctx)
                    if parent not in nodes and not _is_group_function(f, parent, nodes):
                        continue
                    kind = base.attr
                if kind is None:
                    continue
                given = None
                if isinstance(d, ast.Call):
                    given = f.constant(d.args[0]) if d.args else f.constant(f.kwarg(d, "name"))
                handler, inline = f.handler(n, s.owner)
                sym = f.by_node.get(id(n))
                key = ("sym", sym.id) if sym is not None and s.owner is None else ("local", id(n), n.name)
                node = nodes.setdefault(key, Obj(kind=kind))
                node.kind, node.name, node.module, node.line = kind, given or _command_name(n.name, True), s.module, d.lineno
                node.handler, node.inline = handler, inline
                if parent is not None:
                    nodes.setdefault(parent, Obj(kind="group")).children.append((key, None))
                break
        elif isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and n.func.attr == "add_command" and n.args:
            given = f.constant(n.args[1]) if len(n.args) > 1 else f.constant(f.kwarg(n, "name"))
            recv = r.value(n.func.value, s.ctx)
            if recv is not None and recv[0] == "inst" and s.ctx.cls is not None and isinstance(n.func.value, ast.Name):
                class_adds.append((recv[1], n.args[0], given, s.ctx))
                continue
            key = f.key(n.func.value, s.ctx)
            if key is not None:
                later.append((key, n.args[0], given, s.ctx))
    for parent, arg, given, ctx in later:
        child = f.key(arg, ctx)
        if child is not None and parent in nodes:
            nodes[parent].children.append((child, given))
    for cls, arg, given, ctx in class_adds:
        child = f.key(arg, ctx)
        if child is None:
            continue
        for key, node in nodes.items():
            if node.cls is not None and cls in r.mro(node.cls):
                node.children.append((child, given))
    _tree_entries(f, nodes, "click", f.script_names())


def _is_group_function(f: Finder, key: Optional[tuple], nodes: dict) -> bool:
    """Reports whether a key names a function that a click group decorator marks."""
    if key is None or key[0] != "sym":
        return False
    sym = f.p.symbols.get(key[1])
    if sym is None:
        return False
    for d in sym.decorators:
        base = d.func if isinstance(d, ast.Call) else d
        v = f.r.value(base, f.r.ctx_of_module(sym.module))
        if v and v[0] == "ext" and _CLICK_DECORATORS.get(v[1]) == "group":
            return True
        if isinstance(base, ast.Attribute) and base.attr == "group":
            return True
    return False


def typer(f: Finder, sites: list[Site]) -> None:
    """Finds Typer apps, their commands, callbacks and ``add_typer`` calls."""
    apps = _objects(f, sites, {"app": _TYPER}, "_")
    commands: dict[tuple, Obj] = {}
    callbacks: dict[tuple, tuple[str, Module, int, bool]] = {}
    for s in sites:
        n = s.node
        if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef)):
            for d in n.decorator_list:
                if not isinstance(d, ast.Call):
                    continue
                key, method = _receiver(f, d, s.ctx, apps)
                if key is None or method not in {"command", "callback"}:
                    continue
                handler, inline = f.handler(n, s.owner)
                if handler is None:
                    continue
                if method == "callback":
                    callbacks[key] = (handler, s.module, d.lineno, inline)
                    continue
                given = f.constant(d.args[0]) if d.args else f.constant(f.kwarg(d, "name"))
                ckey = ("cmd", handler, d.lineno)
                commands[ckey] = Obj(kind="command", name=given or _command_name(n.name, False), module=s.module, line=d.lineno, handler=handler, inline=inline)
                apps[key].children.append((ckey, None))
        elif isinstance(n, ast.Call):
            key, method = _receiver(f, n, s.ctx, apps)
            if key is not None and method == "add_typer" and n.args:
                child = f.key(n.args[0], s.ctx)
                if child in apps:
                    apps[key].children.append((child, f.constant(f.kwarg(n, "name"))))
    nodes: dict[tuple, Obj] = dict(apps)
    nodes.update(commands)
    scripts = f.script_names()
    mounted = {c for o in apps.values() for c, _ in o.children if c in apps}
    for key, o in apps.items():
        own = [c for c, _ in o.children]
        if key not in mounted and len(own) == 1 and own[0] in commands and key not in callbacks:
            cmd = commands[own[0]]
            o.handler, o.module, o.line, o.inline = cmd.handler, cmd.module, cmd.line, cmd.inline
            o.children = []
            del nodes[own[0]]
        elif not own and key in callbacks:
            o.handler, o.module, o.line, o.inline = callbacks[key]
    _tree_entries(f, nodes, "typer", scripts)


def python_main(f: Finder, sites: list[Site]) -> None:
    """Finds main guards, ``__main__.py`` modules and console scripts."""
    r = f.r
    served = {e["handler"] for e in f.entries if e["kind"] == "cli"}
    for mod in f.p.mapped():
        name = mod.name[: -len(".__main__")] if mod.name.endswith(".__main__") else mod.name
        if mod.guard is not None:
            target = _single_call(r, mod.guard.node.body, r.ctx_of_module(mod))  # type: ignore[attr-defined]
            f.add("cli", "python -m " + name, target or mod.guard.id, mod, mod.guard.line, "python-main")
        elif mod.name.endswith(".__main__") or mod.name == "__main__":
            stmts = [s for s in mod.tree.body if isinstance(s, ast.Expr) and isinstance(s.value, ast.Call)]
            target = _single_call(r, stmts[:1], r.ctx_of_module(mod)) if stmts else None
            if target:
                f.add("cli", "python -m " + name, target, mod, stmts[0].lineno, "python-main")
    if f.p.pyproject_path:
        path = f.p.rel_to_repo(f.p.pyproject_path)
        for script, (target, line) in sorted(f.p.scripts.items()):
            modname, _, attr = target.partition(":")
            v = r.member(modname.strip(), attr.strip().split(".")[0]) if attr else None
            if v is None or v[0] != "func" or v[1].module.is_test or v[1].id in served:
                continue
            if any(_is_click_or_typer(f, d, v[1]) for d in v[1].decorators):
                continue
            f.add("cli", script, v[1].id, v[1].module, line, "python-main", at={"path": path, "line": line})


def _is_click_or_typer(f: Finder, d: ast.expr, sym: Sym) -> bool:
    """Reports whether a decorator makes a function a click or Typer command."""
    base = d.func if isinstance(d, ast.Call) else d
    v = f.r.value(base, f.r.ctx_of_module(sym.module))
    if v and v[0] == "ext" and (v[1].startswith("click.") or v[1].startswith("typer.")):
        return True
    return isinstance(base, ast.Attribute) and base.attr in {"command", "group", "callback"}


def _single_call(r: Resolver, body: list[ast.stmt], ctx: Ctx) -> Optional[str]:
    """Returns the project function that a body of one call statement calls."""
    if len(body) != 1:
        return None
    stmt = body[0]
    expr = stmt.value if isinstance(stmt, ast.Expr) else stmt.exc if isinstance(stmt, ast.Raise) else None
    if isinstance(expr, ast.Call) and expr.args and isinstance(expr.args[0], ast.Call):
        outer = r.value(expr.func, ctx)
        if outer is not None and outer[0] == "ext" and outer[1] in {"sys.exit", "builtins.exit", "builtins.SystemExit", "builtins.quit"}:
            expr = expr.args[0]
    if not isinstance(expr, ast.Call):
        return None
    v = r.value(expr.func, ctx)
    if v is not None and v[0] == "func" and not v[1].module.is_test:
        return v[1].id
    if v is not None and v[0] == "inst":
        return None
    if isinstance(expr.func, ast.Attribute):
        base = r.value(expr.func.value, ctx)
        if base is not None and base[0] == "func" and not base[1].module.is_test:
            return base[1].id
    return None
