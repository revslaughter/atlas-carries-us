# Plan: phase 5, the Python adapter

This plan turns issue #20 into work. Read `AGENTS.md`, `docs/schema.md`,
`docs/adapters.md` and issue #20 first. It corrects the issue's proposal
where the proposal falls short, and answers its open questions.

## Decisions

1. **A Python program, standard library only.** The adapter,
   `adapters/python`, runs on CPython 3.13 and reads code with `ast`. It
   never imports or runs the code under analysis. It needs no package from
   PyPI, so it installs nothing and adds no runtime dependency
   (AGENTS.md, "Ask first", item 3).
2. **Its own resolver, not Pyright or Jedi.** The issue asks to compare
   them. The comparison, and the reasons:
   - Pyright gives the best types, but it offers no stable library API. The
     adapter would have to drive its language server over JSON-RPC, one
     request per name, and Pyright ships as a Node package with its own
     release pace. It also adds a runtime dependency.
   - Jedi resolves names well and runs in-process, but it adds two runtime
     dependencies (Jedi and parso), and it guesses types from code it reads
     inside functions. A guess from inference does not meet the issue's own
     rule for `exact`: "one target from a type that it knows".
   - A resolver on top of `ast` follows Python's scope rules: local, the
     enclosing functions, the module, then imports. It knows a type from an
     annotation, from a constructor call (`x = Store()`), from a return
     annotation (`x = open_store()` where `open_store() -> Store`), and from
     `self`, `cls` and `super()`. That covers most calls in typed code, the
     kind of code that benefits most from a map.
   The adapter's notes report the share of calls that resolved, so a reader
   can judge the map.
3. **Python 3.13 parses every input.** Its `ast` reads the syntax of every
   older Python 3 release, plus `type` statements and type parameters. A file
   that does not parse adds a note and stays out of the map.
4. **No change to the data format.** Every Python word goes to
   `languageKind`. The kinds come from the schema's lists.

## Corrections to the proposal

1. **Container kinds.** The schema has no kind "distribution". The
   distribution takes `kind: "package"` with `languageKind: "distribution"`.
   A Python package takes `kind: "package"` too; a module takes
   `kind: "file"`, with `languageKind: "module"`.
2. **Modifiers for kinds.** The proposal lists `staticmethod` and
   `classmethod` as modifiers only. They also change `languageKind`
   ("static method", "class method"), because a reader of the map reads the
   word on the box first.
3. **Edge names.** The schema names the type edges `field_type`,
   `param_type` and `return_type`, not `field`, `param` and `result`.
4. **`implements`.** The proposal has no `implements` edge. A class that
   names a `typing.Protocol` subclass of the project as a base implements
   it. A protocol takes `kind: "interface"`, `languageKind: "protocol"`.
   Structural matches without a base do not count, as in the TypeScript
   adapter.
5. **Calls with `heuristic` confidence.** A call on a receiver of unknown
   type, such as `store.find(id)` with an untyped `store`, gets an edge only
   when exactly one class of the project defines a method of that name, and
   no built-in type defines one. Such an edge counts as `heuristic`. The
   viewer draws it dashed.
6. **Plugin names.** `main` alone reads as a word of every language, so the
   plain plugin takes the name `python-main`.

## What the adapter reads

1. Every `.py` file under the root that `include` and `exclude` allow. It
   skips folders whose name starts with a dot, `__pycache__`,
   `node_modules`, `site-packages` and virtual environments (a folder that
   holds `pyvenv.cfg`).
2. **Source roots.** A module's dotted name starts at a source root: `src`
   when the root holds a `src` folder, and the root itself. The option
   `sourceRoots` names others, such as `examples/tutorial`. A file takes the
   deepest source root above it.
3. **The distribution** comes from `[project] name` in `pyproject.toml`
   (read with `tomllib`), else `[metadata] name` in `setup.cfg`, else the
   root folder's name.
4. **Test files** match the request's `tests` globs, by default
   `**/test_*.py`, `**/*_test.py`, `**/tests/**`, `**/test/**` and
   `**/conftest.py`. The adapter reads them for the tests, but never maps
   them.
5. **Type stubs.** A `.pyi` file beside a `.py` file stays out of the map;
   the code lives in the `.py` file. A `.pyi` file alone, with no `.py`
   file, stays out too. This answers open question 3: stubs describe code
   that lives elsewhere, as `.d.ts` files do for the TypeScript adapter.

## Containers and IDs

| Container | ID | `kind`, `languageKind` |
| --- | --- | --- |
| The distribution | `py:dist:<name>` | `package`, "distribution" |
| A package: a folder with `__init__.py`, or a folder of modules without one | `py:pkg:<dotted name>` | `package`, "package" or "namespace package"; the unit |
| The modules at the top of a source root | `py:dir:<path>` | `directory`, "folder"; the unit |
| A module | `py:file:<path>` | `file`, "module" |

1. Units stay flat under the distribution, as Go packages sit under their
   module. A package takes its dotted name as its name: `lending.store`.
2. A package's doc comes from its `__init__.py` docstring.
3. A symbol ID joins the module's dotted name and the symbol's path:
   `py:lending.library#Library.borrow`. A module named `__init__` takes its
   package's name: `py:lending#open_library`.
4. A second declaration of one name in one scope gains `@<file>:<line>`;
   the first keeps the plain ID. A property's getter, setter and deleter
   make one symbol, and so do the `@overload` stubs of a function and its
   implementation.

## Symbols

| Python | `kind` | `languageKind` |
| --- | --- | --- |
| `class` | `class` | "class" |
| A `@dataclass` | `class` | "dataclass" |
| A subclass of `Exception` (of the project or the built-ins) | `class` | "exception" |
| A subclass of `Enum`, `IntEnum`, `StrEnum` or `Flag` | `enum` | "enum" |
| A member of such an enum | `constant`, member | "enum member" |
| A subclass of `Protocol` | `interface` | "protocol" |
| A subclass of `TypedDict` | `struct` | "typed dict" |
| A subclass of `NamedTuple` | `struct` | "named tuple" |
| `def` at module level | `function` | "function" |
| `def` in a class | `method`, member | "method" |
| `__init__` | `constructor`, member | "constructor" |
| `@staticmethod`, `@classmethod` | `method`, member | "static method", "class method" |
| `@property`, `@cached_property` | `property`, member | "property", "cached property" |
| An annotated or assigned name in a class body | `field`, member | "class attribute", or "field" in a dataclass, typed dict or named tuple |
| `self.x = …` in `__init__` (or any method) | `field`, member | "instance attribute" |
| A module-level name in `UPPER_CASE`, or annotated `Final` | `constant` | "constant" |
| Any other module-level name | `variable` | "variable" |
| `type X = …`, or `X: TypeAlias = …` | `type_alias` | "type alias" |
| `X = NewType("X", …)` | `type` | "new type" |
| `T = TypeVar("T")`, `ParamSpec`, `TypeVarTuple` | `variable` | "type variable" |
| The body of `if __name__ == "__main__":` | `function` | "main guard" |

1. A nested function or class inside a function makes no symbol. Its calls
   count for the function around it, as in Go.
2. `exported`: a module-level name in `__all__`, when the module has
   `__all__`; else a name that does not start with `_`. A member follows
   its class: it counts as exported when its class does and its own name
   does not start with `_` (dunder names excepted).
3. Modifiers name `async`, `staticmethod`, `classmethod`, `abstract` (from
   `@abstractmethod`, or a class with an abstract method), `generator` (a
   `yield` in the function's own body), `overload` and `final`.
4. Decorators go to `annotations`, as written, with `@`.
5. Signatures leave out `self` and `cls`. `rest` marks `*args` and
   `**kwargs`; `optional` and `default` come from defaults. `text` shows the
   `def` line as written, without the body.
6. PEP 695 type parameters fill `typeParams`: `[T]`.
7. A field's `type` comes from its annotation. `self.store = store` takes
   the annotation of the parameter `store`, and `self.store = Store()` the
   class it builds. Otherwise the field has no type.
8. A constant's `value` holds its source text, cut to 80 characters.

## Docs

1. The adapter reads the docstring of each module, class, function and
   method, and an attribute docstring (a string right after an assignment),
   as Sphinx does.
2. Style detection, in this order: Sphinx fields (`:param x:`, `:returns:`,
   `:raises E:`) give `docstring-sphinx`; a section header followed by a
   line of dashes (`Parameters` / `----------`) gives `docstring-numpy`; a
   section line such as `Args:`, `Returns:`, `Raises:` or `Examples:` gives
   `docstring-google`; anything else gives `plain`.
3. Each style fills `params`, `returns` (and `Yields`), `throws`, `examples`
   and `seeAlso`. `Note:` and other sections join the description with a
   heading.
4. The body text holds reStructuredText. The adapter turns paragraphs,
   bullet and numbered lists, `::` literal blocks, `.. code-block::`,
   doctest blocks (`>>>`), section titles and directives into blocks. A
   directive's text becomes a quote that starts with the directive's name.
   `.. deprecated::` fills `deprecated`, `.. versionadded::` fills `since`,
   and `.. versionchanged::` goes to `tags`.
5. Inline markup: ``` ``code`` ``` becomes code; `*x*` and `**x**` stay;
   `` `text <url>`_ `` becomes a link. A Sphinx role (`:class:`,
   `:func:`, `:meth:`, `:attr:`, `:exc:`, `:mod:`, `:data:`, `:obj:`) and a
   single-backtick name resolve through the module's scope, then the
   project's dotted names, so they become `sym:` links. A name that resolves
   to nothing keeps its text as code. `~` keeps the last part as the label.

## Edges

1. `calls`: each call inside a function or method, resolved by the rules in
   "Decisions", item 2. A call of a class points at its `__init__` (found
   through the bases) and makes no edge when the class has none. A decorator
   of the project counts as a call from the decorated function.
2. A call through a base class or a protocol points at that class's method.
   The viewer follows `overrides` and `implements` edges to the code that
   can run.
3. `extends`: a class to each base class of the project. `implements`: a
   class to each protocol base of the project.
4. `overrides`: a method to the method of the same name in the nearest base
   class of the project, along the method resolution order.
5. `field_type`, `param_type`, `return_type`: from annotations, string
   annotations included. Without an annotation, no edge; the adapter never
   guesses a type from a name.
6. `imports`: a unit to each other unit whose modules it imports. Relative
   imports resolve against the importing module. `from x import *` imports
   `x`, and its names join the scope by `__all__` or by their leading
   underscore. A re-export in `__init__.py` resolves through to the symbol it
   names. Imports inside `try`, `if` and `if TYPE_CHECKING:` count like any
   other.
7. Imports that name no module of the project go to the unit's
   `externalImports` by top-level package name, with a note that counts
   them. The standard library (`sys.stdlib_module_names`) stays out.

## Tests

1. In a test file: a module-level function `test_*`; a method `test_*` in a
   class `Test*`; and every `test*` method of a class that inherits from
   `unittest.TestCase` or `IsolatedAsyncioTestCase`.
2. A test's name follows pytest's node ID: `tests/test_library.py::TestLibrary::test_borrow`.
3. Each project name that a test's body uses (a call, a reference, an
   attribute of a known type) records the test against that symbol.

## Entry-point plugins

Every plugin reads `ast`. A receiver counts only when it comes from the
framework's import (`from flask import Flask`, `import flask`), so a
project's own `route` method never counts.

1. `flask`: `Flask(…)` and `Blueprint(…, url_prefix=…)` objects;
   `@app.route(path, methods=[…])` (GET by default), `@app.get` and the other
   verbs; `app.add_url_rule(path, view_func=f)`; and
   `app.register_blueprint(bp, url_prefix=…)` across modules. An object made
   inside a factory function (`create_app`) counts too; a route inside it
   names the factory as its handler and counts as inline.
2. `fastapi`: `FastAPI(…)` and `APIRouter(prefix=…)`; `@app.get` and the
   other verbs; `@app.api_route(path, methods=[…])`;
   `app.include_router(router, prefix=…)` across modules.
3. `django`: `urlpatterns` lists of `path(…)` and `re_path(…)`, and
   `include("app.urls")` or `include(module)` with its prefix. A function
   view makes `ANY <path>`. A class view (`View.as_view()`) makes one entry
   per HTTP method that the class or its project bases define (`get`,
   `post` and so on), served by that method.
4. `click`: `@click.command()`, `@click.group()`, `@<group>.command()`,
   `@<group>.group()`, and `<group>.add_command(cmd, name)`. A command's name:
   its first argument, else the function's name with `_` turned into `-`
   and the suffixes `_command`, `_cmd`, `_group` and `_grp` cut, as click
   8.2 does. A root group takes the name of the `[project.scripts]` entry
   that names it, else its own name. Names join: `lendd report count`.
5. `typer`: `typer.Typer()` objects; `@app.command()`, `@app.callback()`,
   and `app.add_typer(sub, name=…)`. Names follow click's rule, without the
   suffix cut, as Typer does.
6. `python-main`: the `if __name__ == "__main__":` guard, a package's
   `__main__.py`, and each `[project.scripts]` entry whose function no other
   plugin already serves. A guard whose body makes one call to a project
   function names that function as its handler; any other guard serves
   itself, as a "main guard" symbol. Names: `python -m <module>` for a
   guard or `__main__.py`, and the script's name for a script.

## Fixture and tests

1. `adapters/python/testdata/fixture`: an invented lending library, as the
   other fixtures. A `src` layout distribution `lending` with the packages
   `lending`, `lending.store` (with an abstract base, a protocol and a
   memory store), `lending.web` (Flask, FastAPI and Django modules),
   `lending.cli` (click and Typer), a namespace package, and a `tests`
   folder with pytest and unittest tests. Each docstring style appears.
   The fixture never runs; it needs no framework installed.
2. A golden file `testdata/golden/fixture.json`, rewritten with
   `UPDATE_GOLDEN=1 python3 -m unittest discover -s adapters/python/tests`.
3. Schema validation, with a small validator in the test for the keywords
   that `schema/atlas.schema.json` uses, so the test needs no package.
4. Named facts that read as sentences, and unit tests for the docstring
   parser.

## Corpus

1. `pallets/flask` at 3.1.3 (commit 22d9247, 18 February 2026), as
   `corpus/python/flask`. Its docstrings use Sphinx roles heavily. Its
   command line uses click, and its tutorial app, `examples/tutorial`,
   uses blueprints, so the `flask` and `click` plugins both find entries.
2. This project's own map gains the Python adapter.

## Build and pins

1. `PYTHON_IMAGE` in the `Dockerfile`: `python:3.13.18-slim-bookworm`, by
   digest. The toolchain stage and the runtime image copy CPython from it,
   as they copy Node from the Node image.
2. `.python-version` holds `3.13.18`; `scripts/check-pins.sh` checks that it
   agrees with the `Dockerfile`.
3. `acu` runs `python3 -m acu_adapter_python` with `PYTHONPATH` set to
   `adapters/python` inside this repository. The runtime image installs a
   launcher, `acu-adapter-python`.
4. `plugin_adapter` names `flask`, `fastapi`, `django`, `click`, `typer`
   and `python-main`.

## Limits (open question 2)

The output's notes list these limits, and the help text repeats them:

1. Calls through values that the adapter cannot type do not appear, unless
   one method of the project alone carries the name; such edges count as
   heuristic.
2. A call through a base class or a protocol points at that class's method.
3. Monkey-patching, `setattr`, `getattr` with a computed name, and
   `__getattr__` hide what code runs; the adapter sees only names written in
   the code.
4. A decorator that changes a signature, such as one that wraps a function
   in a class, leaves the signature as written in the `def`.
5. A metaclass, `__init_subclass__` and class decorators that add members
   add nothing to the map; a dataclass's generated `__init__` does not
   appear.
6. Code that runs at module level, outside a function, makes no calls on
   the map, except inside a main guard.
7. Type stubs (`.pyi`) do not appear.
8. A write through another reference or `setattr` stays unseen. The
   resolver ignores statement order, so `del` makes a name unknown
   everywhere.
9. The adapter reads one distribution per root. A monorepo needs one
   language entry per distribution.

## Risks

1. The resolver misses calls in untyped code. The notes report the share of
   resolved calls, so a reader sees the gap.
2. A plugin may miss an unusual registration, such as a route added in a
   loop. The limits in `docs/adapters.md` say so.
