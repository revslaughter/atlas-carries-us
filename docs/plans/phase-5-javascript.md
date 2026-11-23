# Plan: phase 5, JavaScript

This plan turns issue #25 into work. Read `AGENTS.md`, `docs/schema.md`,
`docs/adapters.md` and issue #25 first.

## Decisions

1. **A mode of the TypeScript adapter, not a new adapter.** The issue's
   proposal holds. The compiler API already reads JavaScript: with `allowJs`
   it binds CommonJS exports, `require` calls, prototype assignments,
   `this.x = …` properties and the JSDoc type tags, and the checker resolves
   calls through all of them. A probe on an invented CommonJS file confirmed
   each of these before the work started. A second adapter would copy the
   docs normaliser, the tests index and the Express and Hono plugins.
2. **The config keeps `adapter: typescript`.** acu needs no new adapter name
   and `plugin_adapter` needs no new arm: the JavaScript mode offers no
   plugin of its own, and `express` and `hono` already name the TypeScript
   adapter. A second name for the same program would force acu to route one
   plugin to two adapters.
3. **The mode switches on by itself.** A run reads JavaScript when its map
   holds no TypeScript file and the root holds no `.svelte` file. The option
   `language` (`js` or `ts`) overrides the choice. A Svelte project keeps the
   TypeScript mode, so its IDs stay as they are.
4. **IDs start with `js:` in the JavaScript mode.** The containers and the
   adapter's entry in `meta.adapters` carry the language tag `js`, and
   `meta.languages` holds `["js"]`. The ID grammar stays the same:
   `js:lending-js/lib/shelf#Shelf.add`.
5. **A mixed project makes one run** (open question 1). It runs in the
   TypeScript mode, so every ID starts with `ts:`. Each file container of a
   `.js`, `.mjs`, `.cjs` or `.jsx` file carries `language: "js"`, and
   `meta.languages` lists both tags. Every JavaScript rule below holds for
   such a file too.
6. **`checkJs` stays off** (open question 2). `checkJs` only asks the
   compiler to report errors in JavaScript files. The checker binds and
   infers JavaScript the same way with it off, and the adapter never asks
   for diagnostics. The fixture and the corpus give the same bytes either
   way; section "Checks" records the measurement.
7. **Every edge stays `exact`.** The adapter records only what the checker
   resolves. A call through a value of unknown type resolves to nothing, so
   it adds no edge; the adapter never guesses by name. A note counts the
   calls that name a function of the map but did not resolve, so that a
   reader knows what the map misses.
8. **No change to the data format.** Every JavaScript word goes to
   `languageKind`; every kind comes from the schema's lists.
9. **No new dependency, no new tool.** Node and TypeScript 6.0.3 already sit
   pinned in the `Dockerfile` and the lock file.

## What the adapter reads

1. The config comes from `tsconfig.json`, else from `jsconfig.json`, in the
   root, or from the file that `options.tsconfig` names. The `jsconfig.json`
   fallback, which only the Svelte mode had, now holds for every project.
2. Without a config, the adapter reads every `.ts`, `.tsx`, `.mts`, `.cts`,
   `.js`, `.jsx`, `.mjs` and `.cjs` file under the root. Before, it read only
   the TypeScript ones. A TypeScript project without a config may hold
   build scripts in JavaScript; they now join its map.
3. Without a config, module resolution follows `Bundler`, as before
   (open question on runtimes). `Bundler` resolves `require` and `import`,
   paths without extensions, `index` files, and the `exports` and `imports`
   of `package.json`, so it covers Node, bundlers, Deno-style relative
   imports and Bun. The option `moduleResolution` (`bundler`, `node16`,
   `nodenext` or `node10`) picks another one. Globals come from the
   compiler's default library; browser and Node globals that the project
   does not declare resolve to nothing, which adds no edge.
4. Test files, declaration files and `node_modules` stay out of the map, as
   before.

## Containers

| Container | ID | `kind`, `languageKind` |
| --- | --- | --- |
| The package that `package.json` names | `js:pkg:<name>` | `package`, "package" |
| A folder that holds source files: the unit | `js:dir:<name>/<folder>` | `directory`, "folder" |
| An ES module | `js:file:<name>/<path>` | `file`, "module" |
| A CommonJS module | `js:file:<name>/<path>` | `file`, "CommonJS module" |
| A file with neither | `js:file:<name>/<path>` | `file`, "script" |

A file counts as an ES module when it holds an `import` or `export`
statement. It counts as CommonJS when it uses `require`, `module.exports` or
`exports` and holds no such statement.

## Symbols

The TypeScript table holds, with these additions:

| JavaScript | `kind` | `languageKind` |
| --- | --- | --- |
| A function that `prototype` assignments, `this.x = …` or `@constructor` make a class | `class` | "constructor function" |
| Its body, as the target of `new` | `constructor`, member | "constructor" |
| `F.prototype.m = function …`, or a method of `F.prototype = { … }` | `method`, member | "method" |
| `F.m = function …` on a function or class | `method`, member, `static` | "function" |
| `this.x = …` in a constructor | `property`, member | "property" |
| `F.x = …` with another value | `property`, member, `static` | "property" |
| A top-level `const` object literal that holds a function | `namespace` | "object" |
| A function or method of such an object | `method`, member | "method", "function" or "arrow function" |
| Another property of such an object | `property`, member | "property" |
| `exports.f = function …`, `module.exports.f = …`, or a function in `module.exports = { … }` | `function` | "function" or "arrow function" |
| `module.exports = function …` | `function`, named after the function, else `default` | "function" or "arrow function" |
| `exports.x = …` with another value | `variable` | "export" |
| `@typedef` | `type_alias` | "typedef" |
| A `@property` of a `@typedef` | `property`, member | "property" |
| `@callback` | `type_alias` | "callback" |

1. A declaration counts as exported when the module exports it: an ES
   `export`, `exports.x = …`, `module.exports = x`, or a name in
   `module.exports = { x }`. A `@typedef` or `@callback` in a module counts
   as exported, as TypeScript reads it. Everything in a script counts as
   exported, because a script's names are global.
2. A member counts as exported unless JSDoc marks it `@private` or
   `@protected`, or its name starts with `#`. The compiler reads those tags
   as modifiers in a JavaScript file.
3. `this.x = …` makes a member only in a constructor, or in the constructor
   of an ES class. A class that declares the field keeps that declaration.
4. A mixin (`Object.assign(F.prototype, mixin)`) adds no members; the limits
   list it.

## Types

1. A parameter's type comes from its `@param {T}` tag, as written. A
   bracketed name (`[name]`) or a type `T=` marks it optional; `...T` marks
   it rest.
2. A result's type comes from `@returns {T}` (or `@return`).
3. A variable's or property's type comes from `@type {T}`.
4. A `@typedef` gives a type alias; its `@property` tags give its members,
   with their types.
5. `@template T` gives type parameters.
6. Without a tag, the type comes from the checker's inference, as in
   TypeScript.
7. `field_type`, `param_type` and `return_type` edges follow from these
   types, as in TypeScript. `import("./x").T` and `@import` resolve too.

The docs normaliser keeps dropping `@type`, `@typedef` and `@callback` from
`tags`, because the types now travel in the symbols themselves. A JSDoc
comment that holds a `@typedef` or `@callback` documents that type, never
the declaration below it.

## Modules and edges

1. `imports` follows `import`, `export … from`, `require("x")` and
   `import("x")` with a literal string, anywhere in the file. Outside
   packages go to `externalImports`; Node's built-in modules stay out.
2. `calls`: the checker resolves calls through `require` bindings, prototype
   methods, constructor functions and object methods. The adapter maps each
   declaration the checker returns (a function expression on the right of an
   assignment, a method in an object literal) to its symbol.
3. `new F()` on a constructor function calls `F.constructor`, as it would on
   an ES class.
4. `extends` comes from an ES class's heritage clause, and from
   `@extends`/`@augments` on a class. Prototype chains that code builds at
   run time (`util.inherits`, `Object.create`) add no edge.

## Plugins

`express` and `hono` work in JavaScript as in TypeScript. The receiver test
also follows `require`: `const express = require("express")` marks `app` in
`const app = express()`. Mounts follow `require` too:
`app.use("/admin", require("./admin"))`, and a router that its file exports
with `module.exports = router`.

## Fixture and tests

`adapters/typescript/testdata/javascript`: an invented lending library in
JavaScript, beside the TypeScript fixture. It holds a `jsconfig.json` with
`include`, CommonJS and ES module files, a `.jsx` component, a constructor
function with prototype and static members, an ES class with `this.x`
fields and `@private` members, an object literal module, `@typedef`,
`@callback`, `@template` and `import()` types, an Express app with a mounted
router and an inline handler, a mixin, and tests in `test/`.

1. A golden file, `testdata/golden/javascript.json`, which
   `npm run golden -w adapters/typescript` rewrites.
2. Schema validation of the output.
3. Named facts: the IDs start with `js:`; the kinds and words in the table
   above; the edges through `require` and prototypes; the Express routes;
   the tests index; and, with `options.language: ts`, the `ts:` IDs and the
   `language: "js"` file containers of open question 1.
4. The TypeScript and Svelte golden files stay byte for byte the same.

## Corpus

tj/commander.js v14.0.3 (MIT), pinned as the shallow submodule
`corpus/javascript/commander` (open question 3). Commander ranks among the
most used npm packages. v14 is its last CommonJS line: `require`,
`exports.x = …`, ES classes, and full JSDoc types on almost every method,
with Jest tests. v15 moved to ES modules, which the TypeScript rules already
cover. The config `examples/commander/atlas.yaml` reads `tsconfig.js.json`,
the project's own config for its JavaScript, so its `.ts` type tests stay
out. The tour, marked `drafted`, follows a parse from `program.parse` to an
action handler; each claim comes from the source. The count snapshot sits
in `examples/commander/counts.json`.

## Build and pins

No new pin. The `Dockerfile` already carries Node by digest and the adapter
with TypeScript 6.0.3. `scripts/check.sh` gains the submodule and the
example; `scripts/check-pins.sh` gains nothing to compare, because no new
tool enters. CI runs `scripts/check.sh`, so the example joins CI there.

## Limits

1. Calls through values of unknown type do not appear. In JavaScript
   without JSDoc, a parameter's type often stays unknown.
2. Mixins, `util.inherits`, `Object.create` and other prototype changes at
   run time add no members and no edges.
3. A property that code adds outside a constructor, such as `obj.x = 1`
   inside a method, adds no member, unless the class declares it.
4. Computed names (`exports[name] = …`) and re-assigned exports do not
   appear.
5. Globals that the project does not declare, such as `window` in a project
   without the DOM library, resolve to nothing.

## Risks

1. A large JavaScript code base without JSDoc gives a sparse map: many calls
   stay unresolved. The note that counts them says so.
2. The auto choice of mode changes every ID when a project adds its first
   TypeScript file. `options.language` pins the mode for such a project.
