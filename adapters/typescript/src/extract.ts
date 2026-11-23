// Reads a TypeScript project with the compiler API and builds an Atlas Carries
// Us document (schema version 1) from it.
//
// The extraction runs in four passes:
//
//  1. Declarations: folders, files, and the symbols that each file declares.
//  2. Entry points: the plugins find route registrations. An inline handler
//     at the top level of a module becomes a symbol of its own.
//  3. Relations: type edges, heritage, overrides, imports and calls.
//  4. Docs and tests: doc comments turn into the normal form once every
//     symbol exists, so links resolve to IDs; then the test index.

import fs from "node:fs";
import { builtinModules } from "node:module";
import path from "node:path";
import ts from "typescript";

import { commentText, FILE_TAGS, inline, normalise } from "./docs.js";
import { compileGlobs, type Globs } from "./glob.js";
import { Plugins, type PluginConfig } from "./plugins.js";
import type * as S from "./schema.js";
import { VERSION } from "./schema.js";
import { Svelte } from "./svelte.js";
import { indexTests } from "./tests.js";

/** This adapter's release. */
export const ADAPTER_VERSION = "0.1.0";

/** What the acu command asks the adapter to read. It arrives as JSON on standard input (protocol 1, see docs/adapters.md). */
export interface Request {
  /** The protocol version; this adapter speaks 1. */
  protocol: number;
  /** The repository root; every path in the output is relative to it. The default is the root. */
  repoRoot?: string;
  /** The folder to read: the project's root, which holds its tsconfig. */
  root: string;
  /** Globs of folders to keep; an empty list keeps all. */
  include?: string[];
  /** Globs of folders to leave out. */
  exclude?: string[];
  /** Globs that mark test files; the default is {@link DEFAULT_TESTS}. */
  tests?: string[];
  /** The settings that only this adapter reads. */
  options?: Options;
  /** The entry-point plugins to switch on. */
  entryPoints?: PluginConfig[];
}

/** The settings that only the TypeScript adapter reads. */
export interface Options {
  /** The tsconfig file, relative to the root. The default is tsconfig.json. */
  tsconfig?: string;
}

/** The globs that mark test files when the request names none. */
export const DEFAULT_TESTS = ["**/*.test.*", "**/*.spec.*", "**/__tests__/**"];

/** Matches the files that hold code: TypeScript and JavaScript, as modules or scripts. */
const SCRIPT = /\.(?:[cm]?tsx?|[cm]?jsx?)$/;
/** The symbol kinds that a call can reach, and that a trace can start from. */
const CALLABLE = new Set(["function", "method", "constructor"]);
/** Node's built-in modules, which never count as outside packages. */
const BUILTIN = new Set(builtinModules);

/** Joins the lines of a piece of source text, and drops the spaces that line breaks leave inside brackets. */
export function collapse(s: string): string {
  return s
    .replace(/\s+/g, " ")
    .replace(/([([{<]) /g, "$1")
    .replace(/ ([)\]}>])/g, "$1")
    .replace(/,([)\]}>])/g, "$1")
    .trim();
}

/** Cuts a text to at most n characters. */
export function shorten(s: string, n = 80): string {
  return s.length > n ? s.slice(0, n - 3) + "..." : s;
}

/** Removes parentheses, type assertions and non-null marks around an expression. */
export function unwrap(e: ts.Expression): ts.Expression {
  for (;;) {
    if (ts.isParenthesizedExpression(e) || ts.isAsExpression(e) || ts.isNonNullExpression(e)) e = e.expression;
    else if (ts.isSatisfiesExpression(e) || ts.isTypeAssertionExpression(e)) e = e.expression;
    else return e;
  }
}

/** Reports whether a node is a function written as a value: an arrow function or a function expression. */
function isFunctionLike(e: ts.Node | undefined): e is ts.ArrowFunction | ts.FunctionExpression {
  return !!e && (ts.isArrowFunction(e) || ts.isFunctionExpression(e));
}

/** Reports whether a comment holds a licence header, which documents no code. */
function isLicence(t: string): boolean {
  return (
    /^(Copyright|SPDX-License-Identifier|@license|@preserve)/.test(t) ||
    /Licensed under|license that can be found in the LICENSE file|MIT License/.test(t)
  );
}

/** Reports whether a comment talks to a tool (a linter, the compiler, a test runner) rather than to a reader. */
function isDirective(t: string): boolean {
  return /^(eslint|@ts-|prettier-ignore|tslint|jshint|global |istanbul|c8 |biome-ignore|#region|#endregion|<reference|@jsx|@vitest-environment|@jest-environment|deno-)/.test(
    t,
  );
}

/** A doc comment that waits until every symbol exists, so that its links can resolve. */
interface PendingDoc {
  /** The comment without its markers. */
  raw: string;
  /** The file that holds the comment; its scope resolves link names. */
  sf: ts.SourceFile;
  /** The node that the comment documents. */
  at: ts.Node;
  /** Stores the finished doc on its owner. */
  set: (d: S.Doc) => void;
  /** Stores the doc as one line of inline text instead, for the text of a parameter. */
  inline?: (text: string) => void;
}

/** Holds the state of one run. The plugins and the test index read it too. */
export class Extractor {
  /** The request as it arrived. */
  readonly req: Request;
  /** The absolute folder to read. */
  readonly root: string;
  /** The absolute repository root. */
  readonly repoRoot: string;
  /** The name in the root's package.json, which starts every ID; undefined without one. */
  readonly pkgName: string | undefined;
  /** The program: the project's files and the test files, parsed and bound. */
  program!: ts.Program;
  /** The type checker that resolves every name, type and call. */
  checker!: ts.TypeChecker;
  /** The files that the map holds, sorted by path. */
  readonly files: ts.SourceFile[] = [];
  /** The test files, which feed the test index but never the map. */
  readonly testFiles: ts.SourceFile[] = [];
  /** The file names of {@link Extractor.files}, for a quick check. */
  readonly mapped = new Set<string>();
  /** Every container, by ID. */
  readonly conts = new Map<string, S.Container>();
  /** Every symbol, by ID. */
  readonly syms = new Map<string, S.Symbol>();
  /** Declaration nodes, and the function nodes inside them, to symbol IDs. */
  readonly idOfNode = new Map<ts.Node, string>();
  /** Nodes whose calls count as calls of a symbol: bodies, initializers and inline handlers. */
  readonly callerOfNode = new Map<ts.Node, string>();
  /** The entry points that the plugins found. */
  readonly entries: S.EntryPoint[] = [];
  /** The tests of each symbol ID, keyed by place and name so that a test counts once. */
  readonly testUses = new Map<string, Map<string, S.TestUse>>();
  /** Extra limits of this run, for the output's notes. */
  readonly notes: string[] = [];
  /** Every edge, by "from, to, kind", so that a repeated use raises a count. */
  private readonly edges = new Map<string, S.Edge>();
  /** The doc comments that wait for pass 4. */
  private readonly docs: PendingDoc[] = [];
  /** The symbols that already hold a pending doc, so an overload's second comment never wins. */
  private readonly docQueued = new Set<string>();
  /** The symbols made from each compiler symbol, by category, so overloads and merges join one symbol. */
  private readonly byTsSymbol = new Map<ts.Symbol, Map<string, S.Symbol>>();
  /** Each symbol's path inside its module, such as "Immer.produce", by ID. */
  private readonly pathOf = new Map<string, string>();
  /** Every declared name, so the call pass resolves only calls that may reach the map. */
  private readonly names = new Set<string>();
  /** The declarations that each module exports; null for a script, whose declarations are global. */
  private readonly exportsOf = new Map<ts.SourceFile, Set<ts.Node> | null>();
  /** The local names that each file's imports bind. */
  private readonly aliasesOf = new Map<ts.SourceFile, Set<string>>();
  /** The names in scope at the top of each file, for doc links. */
  private readonly scopeOf = new Map<ts.SourceFile, Map<string, ts.Symbol>>();
  /** The file docs, by file container ID, until a folder claims one. */
  private readonly fileDocs = new Map<string, string>();
  /** Work that needs every symbol to exist: the refs of types. */
  private readonly later: (() => void)[] = [];
  /** The entry-point plugins that the request switches on. */
  private readonly plugins: Plugins;
  /** The folders to keep. */
  private include: Globs;
  /** The folders to leave out. */
  private exclude: Globs;
  /** The files that hold tests. */
  private tests: Globs;
  /** The imports of outside packages that resolved to nothing, for a note. */
  private unresolved = 0;
  /** The Svelte components of the run; undefined when the root holds no .svelte file. */
  svelte: Svelte | undefined;
  /** The files whose IDs keep their extension, because a component already takes the module name without it. */
  private readonly keepExtension = new Set<ts.SourceFile>();

  /** Prepares a run: resolves the paths, compiles the globs and switches on the plugins. Nothing reads code yet. */
  constructor(req: Request) {
    this.req = req;
    this.root = path.resolve(req.root);
    this.repoRoot = path.resolve(req.repoRoot || req.root);
    this.plugins = new Plugins(req.entryPoints ?? []);
    this.include = compileGlobs(req.include ?? []);
    this.exclude = compileGlobs(req.exclude ?? []);
    this.tests = compileGlobs(req.tests?.length ? req.tests : DEFAULT_TESTS);
    this.pkgName = readPackageName(this.root);
  }

  /** Runs every pass and returns the document. */
  run(): S.Document {
    this.load();
    for (const sf of this.files) this.declareFile(sf);
    this.unitDocs();
    for (const sf of this.files) this.plugins.scan(this, sf);
    this.plugins.finish(this);
    this.relations();
    for (const sf of this.files) {
      this.imports(sf);
      this.walkCalls(sf);
    }
    this.resolveDocs();
    indexTests(this, this.testFiles);
    return this.document();
  }

  /* Loading. */

  private load() {
    const opt = this.req.options ?? {};
    let configPath = path.resolve(this.root, opt.tsconfig ?? "tsconfig.json");
    const all = walk(this.root, r => this.exclude.match(r));
    const components = all.filter(f => f.endsWith(".svelte"));
    if (components.length) this.svelte = new Svelte(components, f => this.rel(f));
    // A project with components may hold its config in jsconfig.json, as
    // svelte-check allows.
    const jsconfig = path.join(this.root, "jsconfig.json");
    if (this.svelte && !opt.tsconfig && !fs.existsSync(configPath) && fs.existsSync(jsconfig)) configPath = jsconfig;
    let options: ts.CompilerOptions;
    let rootNames: string[];
    if (fs.existsSync(configPath)) {
      let parsed = parseConfig(configPath);
      // SvelteKit's generated config, which the project extends, lets
      // JavaScript in; without it, read the config again with allowJs.
      if (this.svelte && this.missingExtends(configPath, parsed)) parsed = parseConfig(configPath, { allowJs: true });
      options = parsed.options;
      rootNames = [...parsed.fileNames];
      if (!rootNames.length && parsed.projectReferences?.length) {
        // A solution-style tsconfig lists other projects; read their files.
        for (const ref of parsed.projectReferences) {
          const p = path.resolve(ref.path);
          const file = fs.statSync(p).isDirectory() ? path.join(p, "tsconfig.json") : p;
          if (!fs.existsSync(file)) continue;
          const sub = parseConfig(file);
          if (!rootNames.length) options = sub.options;
          rootNames.push(...sub.fileNames);
        }
      }
    } else if (opt.tsconfig) {
      throw new Error(
        `the tsconfig ${opt.tsconfig} does not exist under ${this.root}; fix options.tsconfig in atlas.yaml`,
      );
    } else {
      options = {
        target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.ESNext,
        moduleResolution: ts.ModuleResolutionKind.Bundler,
        jsx: ts.JsxEmit.Preserve,
        strict: true,
      };
      // A project with components reads its JavaScript too, as svelte-check does.
      const source = this.svelte ? /\.[cm]?[jt]sx?$/ : /\.[cm]?tsx?$/;
      rootNames = all.filter(f => source.test(f) && !/\.d\.[cm]?ts$/.test(f));
    }
    options = {
      ...options,
      noEmit: true,
      allowJs: true,
      checkJs: false,
      skipLibCheck: true,
      composite: false,
      incremental: false,
      declaration: false,
      declarationMap: false,
      emitDeclarationOnly: false,
      sourceMap: false,
    };
    let host = ts.createCompilerHost(options, true);
    host.jsDocParsingMode = ts.JSDocParsingMode.ParseAll;
    if (this.svelte) {
      this.svelte.configure(options, this.root);
      rootNames.push(...this.svelte.virtualNames(), ...this.svelte.shims());
      host = this.svelte.wrap(host, options);
    }
    const first = ts.createProgram({ rootNames, options, host });
    const tests = all.filter(f => SCRIPT.test(f) && !/\.d\.[cm]?ts$/.test(f) && this.tests.match(this.relRoot(f)));
    this.program = tests.length
      ? ts.createProgram({ rootNames: [...rootNames, ...tests], options, host, oldProgram: first })
      : first;
    this.checker = this.program.getTypeChecker();
    const keep: ts.SourceFile[] = [];
    for (const sf0 of first.getSourceFiles()) {
      const sf = this.program.getSourceFile(sf0.fileName);
      if (!sf || sf.isDeclarationFile || !SCRIPT.test(sf.fileName)) continue;
      const r = this.relRoot(sf.fileName);
      if (r.startsWith("..") || path.isAbsolute(r) || r.split("/").includes("node_modules")) continue;
      if (this.tests.match(r)) continue;
      const dirRoot = path.posix.dirname(r);
      const dirRepo = path.posix.dirname(this.rel(sf.fileName));
      const d = dirRoot === "." ? "" : dirRoot;
      if (this.exclude.match(d) || this.exclude.match(dirRepo)) continue;
      if (this.include.size && !this.include.match(d) && !this.include.match(dirRepo)) continue;
      keep.push(sf);
    }
    if (!keep.length)
      throw new Error(
        `no TypeScript source files found under ${this.root}; check the root, include and exclude in atlas.yaml`,
      );
    keep.sort((a, b) => cmp(this.rel(a.fileName), this.rel(b.fileName)));
    for (const sf of keep) {
      this.files.push(sf);
      this.mapped.add(sf.fileName);
    }
    for (const f of tests.sort()) {
      const sf = this.program.getSourceFile(f);
      if (sf) this.testFiles.push(sf);
    }
    if (this.svelte) this.moduleNameClashes();
  }

  /** Adds a note, and returns true, when the tsconfig extends a file that does not exist yet, such as the config that `svelte-kit sync` writes. TypeScript then reads the tsconfig without it. */
  private missingExtends(file: string, parsed: ts.ParsedCommandLine): boolean {
    const msgs = parsed.errors
      .filter(d => d.code === 6053 || d.code === 5083)
      .map(d => ts.flattenDiagnosticMessageText(d.messageText, " "));
    if (!msgs.length) return false;
    const raw = (parsed.raw as { extends?: unknown } | undefined)?.extends;
    const list = (Array.isArray(raw) ? raw : [raw]).filter((x): x is string => typeof x === "string");
    const missing = list.filter(x => msgs.some(m => m.includes(x) || m.includes(path.resolve(path.dirname(file), x))));
    this.notes.push(
      `${this.rel(file)} extends ${(missing.length ? missing : list).join(", ")}, which does not resolve without an install and svelte-kit sync; the adapter reads the config without it, with allowJs and the compiler options that svelte-check forces.`,
    );
    return true;
  }

  /** Finds the files whose module name a component already takes (Button.svelte.ts beside Button.svelte), keeps their extension in their IDs, and adds a note for each. */
  private moduleNameClashes() {
    const svelte = this.svelte!;
    const taken = new Set(this.files.filter(sf => svelte.has(sf.fileName)).map(sf => this.relRoot(sf.fileName)));
    for (const sf of this.files) {
      if (svelte.has(sf.fileName)) continue;
      const r = this.relRoot(sf.fileName);
      const m = r.replace(/\.[cm]?[jt]sx?$/, "");
      if (m === r || !taken.has(m)) continue;
      this.keepExtension.add(sf);
      this.notes.push(
        `${this.rel(sf.fileName)} and the component ${path.posix.basename(m)} share a module name; the component keeps it, and the IDs of ${path.posix.basename(r)} keep its extension.`,
      );
    }
  }

  /* Paths and IDs. See docs/schema.md, section 3. */

  /** The path of a file relative to the repository root, with forward slashes. */
  rel(file: string): string {
    const r = path
      .relative(this.repoRoot, this.svelte ? this.svelte.real(file) : file)
      .split(path.sep)
      .join("/");
    return r || ".";
  }

  /** The path of a file relative to the language root. A component's virtual name gives the path of its .svelte file. */
  relRoot(file: string): string {
    return path
      .relative(this.root, this.svelte ? this.svelte.real(file) : file)
      .split(path.sep)
      .join("/");
  }

  /** The first line of a node, counting from 1. In a component, the line comes from the .svelte file. */
  line(node: ts.Node, sf = node.getSourceFile()): number {
    const pos = node.getStart(sf);
    return this.svelte?.lineOf(sf, pos) ?? sf.getLineAndCharacterOfPosition(pos).line + 1;
  }

  /** The last line of a node, counting from 1. In a component, the line comes from the .svelte file, and never falls before the first. */
  private endLine(node: ts.Node, sf: ts.SourceFile): number {
    const mapped = this.svelte?.lineOf(sf, Math.max(node.getStart(sf), node.getEnd() - 1));
    if (mapped !== undefined) return Math.max(mapped, this.line(node, sf));
    return sf.getLineAndCharacterOfPosition(node.getEnd()).line + 1;
  }

  /** The module part of a file's IDs: the package name and the file's path without its extension. A .svelte file keeps its whole name. */
  private moduleKey(sf: ts.SourceFile): string {
    const r = this.relRoot(sf.fileName);
    const m = this.keepExtension.has(sf) ? r : r.replace(/\.[cm]?[jt]sx?$/, "");
    return this.pkgName ? `${this.pkgName}/${m}` : m;
  }

  /** The ID of a symbol in a file: the module path, "#", and the symbol's path in the module. */
  symId(sf: ts.SourceFile, p: string): string {
    return `ts:${this.moduleKey(sf)}#${p}`;
  }

  /** The ID of a file's container. */
  fileId(sf: ts.SourceFile): string {
    return `ts:file:${this.pkgName ? this.pkgName + "/" : ""}${this.relRoot(sf.fileName)}`;
  }

  /** The ID of the folder that holds a file: its unit. */
  dirIdOf(sf: ts.SourceFile): string {
    const d = path.posix.dirname(this.relRoot(sf.fileName));
    const key = this.pkgName ? (d === "." ? this.pkgName : `${this.pkgName}/${d}`) : d;
    return `ts:dir:${key}`;
  }

  /** Adds a symbol. A second symbol with the same ID gains a suffix: "@file:line". */
  addSym(s: S.Symbol, p: string): S.Symbol {
    if (this.syms.has(s.id)) s.id = `${s.id}@${path.posix.basename(s.location.path)}:${s.location.line}`;
    this.syms.set(s.id, s);
    this.pathOf.set(s.id, p);
    this.names.add(s.name);
    return s;
  }

  /** The symbol ID of a compiler symbol, through imports and re-exports, or undefined. */
  idOfSymbol(sym: ts.Symbol | undefined): string | undefined {
    if (!sym) return undefined;
    if (sym.flags & ts.SymbolFlags.Alias) {
      try {
        sym = this.checker.getAliasedSymbol(sym);
      } catch {
        return undefined;
      }
    }
    for (const d of sym.declarations ?? []) {
      const id = this.idOfNode.get(d);
      if (id) return id;
    }
    const ex = this.checker.getExportSymbolOfSymbol(sym);
    if (ex !== sym) for (const d of ex.declarations ?? []) if (this.idOfNode.has(d)) return this.idOfNode.get(d);
    return undefined;
  }

  /** Reports whether a symbol ID names a function, method or constructor. */
  isCallable(id: string | undefined): boolean {
    return !!id && CALLABLE.has(this.syms.get(id)?.kind ?? "");
  }

  /** Records one use. A repeated use raises the edge's count and keeps its first line. */
  addEdge(from: string, to: string, kind: S.EdgeKind, line: number, confidence: S.Edge["confidence"] = "exact") {
    // A call from a function to itself (recursion) counts; other self-edges do not.
    if (!from || !to || (from === to && kind !== "calls")) return;
    const k = `${from}\0${to}\0${kind}`;
    const e = this.edges.get(k);
    if (e) {
      e.count = (e.count ?? 1) + 1;
      if (line > 0 && (!e.line || line < e.line)) e.line = line;
      if (confidence === "exact") e.confidence = "exact";
      return;
    }
    const edge: S.Edge = { from, to, kind, confidence };
    if (line > 0) edge.line = line;
    this.edges.set(k, edge);
  }

  /* Pass 1: declarations. */

  private declareFile(sf: ts.SourceFile) {
    const r = this.relRoot(sf.fileName);
    const dRoot = path.posix.dirname(r);
    const dirAbs = path.dirname(sf.fileName);
    const dirId = this.dirIdOf(sf);
    if (this.pkgName && !this.conts.has(`ts:pkg:${this.pkgName}`)) {
      this.conts.set(`ts:pkg:${this.pkgName}`, {
        id: `ts:pkg:${this.pkgName}`,
        kind: "package",
        languageKind: "package",
        name: this.pkgName,
        path: this.rel(this.root),
        language: "ts",
      });
    }
    if (!this.conts.has(dirId)) {
      const c: S.Container = {
        id: dirId,
        kind: "directory",
        languageKind: "folder",
        name: dRoot === "." ? (this.pkgName ?? path.basename(this.root)) : dRoot,
        path: this.rel(dirAbs),
        language: "ts",
        unit: true,
      };
      if (this.pkgName) c.parent = `ts:pkg:${this.pkgName}`;
      this.conts.set(dirId, c);
    }
    const fc: S.Container = {
      id: this.fileId(sf),
      kind: "file",
      languageKind: ts.isExternalModule(sf) ? "module" : "script",
      name: path.posix.basename(r),
      path: this.rel(sf.fileName),
      parent: dirId,
      language: "ts",
    };
    this.conts.set(fc.id, fc);
    const fd = this.svelte?.has(sf.fileName) ? undefined : this.fileDoc(sf);
    if (fd) this.fileDocs.set(fc.id, fd);
    if (this.svelte?.has(sf.fileName)) this.svelte.declare(this, sf, fc.id);
    else this.statements(sf, sf.statements, fc.id, "", undefined);
  }

  /** Finds the comment that describes a whole file: the first comment that no declaration owns. */
  private fileDoc(sf: ts.SourceFile): string | undefined {
    const text = sf.getFullText();
    const first = sf.statements[0];
    const firstStart = first ? first.getStart(sf) : text.length;
    // From position 0, TypeScript skips a "#!" line by itself. From any other
    // position, it collects comments only after a line break.
    const ranges = (ts.getLeadingCommentRanges(text, 0) ?? []).filter(r => r.end <= firstStart);
    // Join neighbouring line comments into one group.
    const groups: { pos: number; end: number; raw: string; block: boolean }[] = [];
    for (const r of ranges) {
      const raw = text.slice(r.pos, r.end);
      const last = groups[groups.length - 1];
      const block = r.kind === ts.SyntaxKind.MultiLineCommentTrivia;
      if (last && !block && !last.block && /^[ \t]*\r?\n[ \t]*$/.test(text.slice(last.end, r.pos))) {
        last.raw += "\n" + raw;
        last.end = r.end;
      } else groups.push({ pos: r.pos, end: r.end, raw, block });
    }
    for (let i = 0; i < groups.length; i++) {
      const g = groups[i];
      const t = commentText(g.raw);
      if (!t || isDirective(t) || isLicence(t)) continue;
      const lastBefore = i === groups.length - 1;
      const gap = text.slice(g.end, firstStart);
      const touches = first !== undefined && lastBefore && !/\n[ \t]*\r?\n/.test(gap);
      if (FILE_TAGS.test(t)) return t;
      if (g.raw.startsWith("/**") && lastBefore && first) return undefined; // the doc of the first declaration
      if (touches) return undefined;
      return t;
    }
    return undefined;
  }

  /**
   * Gives each folder a doc: the doc of the file that carries
   * `@packageDocumentation`, or else the doc of its index file. That file's
   * own container keeps no doc, as a Go package's doc file keeps none.
   */
  private unitDocs() {
    const owner = new Map<string, ts.SourceFile>();
    for (const sf of this.files) {
      const d = this.fileDocs.get(this.fileId(sf));
      if (!d) continue;
      const dir = this.dirIdOf(sf);
      if (/(^|\n)\s*@packageDocumentation\b/.test(d)) owner.set(dir, sf);
      else if (!owner.has(dir) && /^(index|mod)\.[cm]?[jt]sx?$/.test(path.basename(sf.fileName))) owner.set(dir, sf);
    }
    for (const sf of this.files) {
      const fid = this.fileId(sf);
      const d = this.fileDocs.get(fid);
      if (!d) continue;
      const dir = this.dirIdOf(sf);
      const target = owner.get(dir) === sf ? this.conts.get(dir)! : this.conts.get(fid)!;
      this.queue(d, sf, sf, x => (target.doc = x));
    }
  }

  private exportedDecls(sf: ts.SourceFile): Set<ts.Node> | null {
    if (this.exportsOf.has(sf)) return this.exportsOf.get(sf)!;
    const ms = this.checker.getSymbolAtLocation(sf);
    let set: Set<ts.Node> | null = null;
    if (ms) {
      set = new Set();
      for (const e of this.checker.getExportsOfModule(ms)) {
        for (const d of e.declarations ?? []) set.add(d);
        if (e.flags & ts.SymbolFlags.Alias) {
          try {
            for (const d of this.checker.getAliasedSymbol(e).declarations ?? []) set.add(d);
          } catch {
            // An export of something that does not resolve.
          }
        }
      }
    }
    this.exportsOf.set(sf, set);
    return set;
  }

  /** Reports whether code outside the module may use a top-level declaration. A script's declarations are global. */
  private isExported(sf: ts.SourceFile, decl: ts.Node, ns: boolean | undefined): boolean {
    if (ns !== undefined)
      return ns && !!(ts.getCombinedModifierFlags(decl as ts.Declaration) & ts.ModifierFlags.Export);
    const set = this.exportedDecls(sf);
    return set === null || set.has(decl);
  }

  private tsSymbol(decl: ts.Declaration): ts.Symbol | undefined {
    const name = ts.getNameOfDeclaration(decl);
    return (name && this.checker.getSymbolAtLocation(name)) || (decl as { symbol?: ts.Symbol }).symbol;
  }

  /** Returns the symbol for a declaration, merging declarations of one compiler symbol that belong together (overloads, interface merges). */
  private topSymbol(
    sf: ts.SourceFile,
    decl: ts.Declaration,
    category: string,
    p: string,
    build: () => S.Symbol,
  ): { s: S.Symbol; fresh: boolean } {
    const tsSym = this.tsSymbol(decl);
    let m = tsSym && this.byTsSymbol.get(tsSym);
    if (tsSym && !m) this.byTsSymbol.set(tsSym, (m = new Map()));
    const old = m?.get(category);
    if (old && old.location.path === this.rel(sf.fileName)) {
      old.location.endLine = Math.max(old.location.endLine ?? 0, this.endLine(decl, sf));
      this.idOfNode.set(decl, old.id);
      return { s: old, fresh: false };
    }
    const s = this.addSym(build(), p);
    m?.set(category, s);
    this.idOfNode.set(decl, s.id);
    return { s, fresh: true };
  }

  private loc(sf: ts.SourceFile, node: ts.Node, end = true): S.Location {
    const l: S.Location = { path: this.rel(sf.fileName), line: this.line(node, sf) };
    if (end) l.endLine = this.endLine(node, sf);
    return l;
  }

  /**
   * Declares the symbols of a list of statements under a parent: a file, a
   * namespace or a component. Every member path starts with `prefix`. `ns`
   * holds whether the enclosing namespace counts as exported; undefined
   * means the top level of a module, where the module's exports decide.
   */
  statements(
    sf: ts.SourceFile,
    list: readonly ts.Statement[],
    parent: string,
    prefix: string,
    ns: boolean | undefined,
  ) {
    for (const st of list) {
      if (ts.isFunctionDeclaration(st)) {
        const name = st.name?.text ?? "default";
        const p = prefix + name;
        const { s, fresh } = this.topSymbol(sf, st, "function", p, () => ({
          id: this.symId(sf, p),
          kind: "function",
          languageKind: "function",
          name,
          parent,
          exported: this.isExported(sf, st, ns),
          location: this.loc(sf, st),
          signature: this.signature(sf, st, "function", name),
          modifiers: this.modifiers(st),
        }));
        if (fresh && !s.modifiers?.length) delete s.modifiers;
        if (st.body) this.callerOfNode.set(st, s.id);
        this.queueDecl(st, sf, s);
      } else if (ts.isClassDeclaration(st)) {
        const name = st.name?.text ?? "default";
        const p = prefix + name;
        const { s } = this.topSymbol(sf, st, "class", p, () =>
          this.typeSymbol(sf, st, p, name, parent, "class", "class", this.isExported(sf, st, ns)),
        );
        const dec = this.decorators(st);
        if (dec.length) s.annotations = dec;
        this.queueDecl(st, sf, s);
        this.members(sf, s, p, st.members);
      } else if (ts.isInterfaceDeclaration(st)) {
        const name = st.name.text;
        const p = prefix + name;
        const tsSym = this.tsSymbol(st);
        const cat = tsSym && tsSym.flags & ts.SymbolFlags.Class ? "class" : "interface";
        const { s } = this.topSymbol(sf, st, cat, p, () =>
          this.typeSymbol(sf, st, p, name, parent, "interface", "interface", this.isExported(sf, st, ns)),
        );
        this.queueDecl(st, sf, s);
        this.members(sf, s, p, st.members);
      } else if (ts.isTypeAliasDeclaration(st)) {
        const name = st.name.text;
        const p = prefix + name;
        const { s } = this.topSymbol(sf, st, "type", p, () => {
          const t = this.typeSymbol(sf, st, p, name, parent, "type_alias", "type alias", this.isExported(sf, st, ns));
          t.type = {
            text: shorten(collapse(st.type.getText(sf)), 120),
            refs: this.refsLater(() => this.typeNodeRefs(st.type)),
          };
          return t;
        });
        this.queueDecl(st, sf, s);
        if (ts.isTypeLiteralNode(st.type)) this.members(sf, s, p, st.type.members);
      } else if (ts.isEnumDeclaration(st)) {
        const name = st.name.text;
        const p = prefix + name;
        const { s } = this.topSymbol(sf, st, "enum", p, () =>
          this.typeSymbol(sf, st, p, name, parent, "enum", "enum", this.isExported(sf, st, ns)),
        );
        this.queueDecl(st, sf, s);
        for (const m of st.members) {
          const mn = ts.isIdentifier(m.name) || ts.isStringLiteral(m.name) ? m.name.text : m.name.getText(sf);
          const v = this.checker.getConstantValue(m);
          const ms = this.addSym(
            {
              id: this.symId(sf, `${p}.${mn}`),
              kind: "constant",
              languageKind: "enum member",
              name: mn,
              parent: s.id,
              exported: s.exported,
              location: this.loc(sf, m, false),
              value:
                v !== undefined
                  ? typeof v === "string"
                    ? JSON.stringify(v)
                    : String(v)
                  : m.initializer
                    ? shorten(collapse(m.initializer.getText(sf)))
                    : undefined,
            },
            `${p}.${mn}`,
          );
          if (ms.value === undefined) delete ms.value;
          this.idOfNode.set(m, ms.id);
          this.queueDecl(m, sf, ms);
        }
      } else if (ts.isModuleDeclaration(st) && ts.isIdentifier(st.name)) {
        this.namespace(sf, st, parent, prefix, ns);
      } else if (ts.isVariableStatement(st)) {
        this.variables(sf, st, parent, prefix, ns);
      } else if (ts.isExportAssignment(st) && !st.isExportEquals) {
        const e = unwrap(st.expression);
        if (isFunctionLike(e)) {
          const p = prefix + "default";
          const s = this.addSym(
            {
              id: this.symId(sf, p),
              kind: "function",
              languageKind: ts.isArrowFunction(e) ? "arrow function" : "function",
              name: "default",
              parent,
              exported: true,
              location: this.loc(sf, st),
              signature: this.signature(sf, e, ts.isArrowFunction(e) ? "arrow" : "function", "default"),
            },
            p,
          );
          this.idOfNode.set(e, s.id);
          this.callerOfNode.set(e, s.id);
          this.queueDecl(st, sf, s);
        }
      }
    }
  }

  private namespace(
    sf: ts.SourceFile,
    st: ts.ModuleDeclaration,
    parent: string,
    prefix: string,
    ns: boolean | undefined,
  ) {
    const name = st.name.getText(sf);
    const p = prefix + name;
    const tsSym = this.tsSymbol(st);
    const merged = tsSym && tsSym.flags & (ts.SymbolFlags.Class | ts.SymbolFlags.Function | ts.SymbolFlags.Enum);
    const cat = merged
      ? tsSym!.flags & ts.SymbolFlags.Class
        ? "class"
        : tsSym!.flags & ts.SymbolFlags.Enum
          ? "enum"
          : "function"
      : "namespace";
    const existing = tsSym && this.byTsSymbol.get(tsSym)?.get(cat);
    let owner: S.Symbol;
    if (existing) owner = existing;
    else {
      owner = this.topSymbol(sf, st, cat, p, () => ({
        id: this.symId(sf, p),
        kind: "namespace",
        languageKind: "namespace",
        name,
        parent,
        exported: this.isExported(sf, st, ns),
        location: this.loc(sf, st),
      })).s;
      this.queueDecl(st, sf, owner);
    }
    let body = st.body;
    let path_ = p;
    while (body && ts.isModuleDeclaration(body)) {
      path_ += "." + body.name.getText(sf);
      body = body.body;
    }
    if (body && ts.isModuleBlock(body)) this.statements(sf, body.statements, owner.id, path_ + ".", owner.exported);
  }

  private variables(
    sf: ts.SourceFile,
    st: ts.VariableStatement,
    parent: string,
    prefix: string,
    ns: boolean | undefined,
  ) {
    const isConst = !!(st.declarationList.flags & ts.NodeFlags.Const);
    const group = `${path.posix.basename(this.rel(sf.fileName))}:${this.line(st, sf)}`;
    for (const d of st.declarationList.declarations) {
      if (!ts.isIdentifier(d.name)) continue;
      const name = d.name.text;
      const p = prefix + name;
      const init = d.initializer && unwrap(d.initializer);
      const exported =
        ns !== undefined
          ? ns && !!(ts.getCombinedModifierFlags(d) & ts.ModifierFlags.Export)
          : this.isExported(sf, d, undefined);
      let s: S.Symbol;
      if (isFunctionLike(init)) {
        s = this.topSymbol(sf, d, "variable", p, () => ({
          id: this.symId(sf, p),
          kind: "function",
          languageKind: ts.isArrowFunction(init) ? "arrow function" : "function",
          name,
          parent,
          exported,
          location: this.loc(sf, st),
          signature: this.signature(sf, init, ts.isArrowFunction(init) ? "arrow" : "function", name),
        })).s;
        const mods = this.modifiers(init);
        if (mods.length) s.modifiers = mods;
        this.idOfNode.set(init, s.id);
        this.callerOfNode.set(init, s.id);
      } else {
        s = this.topSymbol(sf, d, "variable", p, () => {
          const v: S.Symbol = {
            id: this.symId(sf, p),
            kind: isConst ? "constant" : "variable",
            languageKind: isConst ? "constant" : "variable",
            name,
            parent,
            exported,
            location: this.loc(sf, d),
            group,
            type: this.typeOf(sf, d, d.type),
          };
          if (d.initializer) v.value = shorten(collapse(d.initializer.getText(sf)));
          return v;
        }).s;
        if (d.initializer) this.callerOfNode.set(d.initializer, s.id);
        if (d.type && ts.isFunctionTypeNode(d.type)) this.idOfNode.set(d.type, s.id);
      }
      this.queueDecl(d, sf, s);
    }
  }

  /** Builds the symbol of a class, interface, type alias or enum. */
  private typeSymbol(
    sf: ts.SourceFile,
    node: ts.ClassDeclaration | ts.InterfaceDeclaration | ts.TypeAliasDeclaration | ts.EnumDeclaration,
    p: string,
    name: string,
    parent: string,
    kind: S.SymbolKind,
    languageKind: string,
    exported: boolean,
  ): S.Symbol {
    const s: S.Symbol = {
      id: this.symId(sf, p),
      kind,
      languageKind,
      name,
      parent,
      exported,
      location: this.loc(sf, node),
    };
    if (!ts.isEnumDeclaration(node) && node.typeParameters?.length)
      s.typeParams = "<" + node.typeParameters.map(t => collapse(t.getText(sf))).join(", ") + ">";
    const mods = this.modifiers(node);
    if (mods.length) s.modifiers = mods;
    return s;
  }

  private modifiers(node: ts.Node): string[] {
    const f = ts.canHaveModifiers(node) ? ts.getCombinedModifierFlags(node as ts.Declaration) : 0;
    const out: string[] = [];
    const add = (flag: ts.ModifierFlags, word: string) => {
      if (f & flag) out.push(word);
    };
    add(ts.ModifierFlags.Private, "private");
    add(ts.ModifierFlags.Protected, "protected");
    add(ts.ModifierFlags.Static, "static");
    add(ts.ModifierFlags.Abstract, "abstract");
    add(ts.ModifierFlags.Async, "async");
    add(ts.ModifierFlags.Readonly, "readonly");
    add(ts.ModifierFlags.Override, "override");
    add(ts.ModifierFlags.Ambient, "declare");
    add(ts.ModifierFlags.Default, "default");
    if (ts.isEnumDeclaration(node)) add(ts.ModifierFlags.Const, "const");
    if (
      (ts.isFunctionDeclaration(node) || ts.isMethodDeclaration(node) || ts.isFunctionExpression(node)) &&
      node.asteriskToken
    )
      out.push("generator");
    return out;
  }

  private decorators(node: ts.Node): string[] {
    if (!ts.canHaveDecorators(node)) return [];
    return (ts.getDecorators(node) ?? []).map(d => collapse(d.getText()));
  }

  private memberName(m: ts.ClassElement | ts.TypeElement, sf: ts.SourceFile): string | undefined {
    const n = m.name;
    if (!n) return ts.isConstructorDeclaration(m) ? "constructor" : undefined;
    if (ts.isIdentifier(n) || ts.isPrivateIdentifier(n) || ts.isStringLiteral(n) || ts.isNumericLiteral(n))
      return n.text;
    return n.getText(sf);
  }

  /** Declares the members of a class, interface or object type. */
  private members(
    sf: ts.SourceFile,
    owner: S.Symbol,
    ownerPath: string,
    list: readonly (ts.ClassElement | ts.TypeElement)[],
  ) {
    const seen = new Map<ts.Symbol, S.Symbol>();
    for (const m of list) {
      if (ts.isIndexSignatureDeclaration(m) || ts.isClassStaticBlockDeclaration(m) || ts.isSemicolonClassElement(m))
        continue;
      if (ts.isCallSignatureDeclaration(m) || ts.isConstructSignatureDeclaration(m)) continue;
      const name = this.memberName(m, sf);
      if (name === undefined) continue;
      const tsSym = (m as { symbol?: ts.Symbol }).symbol;
      const prior = tsSym && seen.get(tsSym);
      const p = `${ownerPath}.${name}`;
      const mods = this.modifiers(m);
      if ((m as ts.PropertyDeclaration).questionToken) mods.push("optional");
      const hidden = mods.includes("private") || mods.includes("protected") || name.startsWith("#");
      if (prior) {
        // An overload, or the other half of a get/set pair.
        this.idOfNode.set(m, prior.id);
        prior.location.endLine = Math.max(prior.location.endLine ?? 0, this.endLine(m, sf));
        if (ts.isGetAccessor(m) || ts.isSetAccessor(m)) {
          const word = ts.isGetAccessor(m) ? "get" : "set";
          if (!prior.modifiers?.includes(word)) (prior.modifiers ??= []).push(word);
          if (ts.isGetAccessor(m) && m.type) prior.type = this.typeOf(sf, m, m.type);
        }
        if (ts.isFunctionLike(m) && "body" in m && m.body) this.callerOfNode.set(m, prior.id);
        this.queueDecl(m, sf, prior);
        continue;
      }
      const s: S.Symbol = {
        id: this.symId(sf, p),
        kind: "property",
        languageKind: "property",
        name,
        parent: owner.id,
        exported: !hidden,
        location: this.loc(sf, m),
      };
      if (ts.isConstructorDeclaration(m)) {
        s.kind = "constructor";
        s.languageKind = "constructor";
        s.signature = this.signature(sf, m, "constructor", name);
      } else if (ts.isMethodDeclaration(m) || ts.isMethodSignature(m)) {
        s.kind = "method";
        s.languageKind = "method";
        s.signature = this.signature(sf, m, "method", name);
      } else if (ts.isGetAccessor(m) || ts.isSetAccessor(m)) {
        s.languageKind = "accessor";
        mods.push(ts.isGetAccessor(m) ? "get" : "set");
        s.type = ts.isGetAccessor(m)
          ? this.typeOf(sf, m, m.type)
          : this.typeOf(sf, m.parameters[0] ?? m, m.parameters[0]?.type);
      } else if (ts.isPropertyDeclaration(m) || ts.isPropertySignature(m)) {
        const init = ts.isPropertyDeclaration(m) && m.initializer ? unwrap(m.initializer) : undefined;
        if (isFunctionLike(init)) {
          s.kind = "method";
          s.languageKind = ts.isArrowFunction(init) ? "arrow function" : "function";
          s.signature = this.signature(sf, init, "property", name);
          this.idOfNode.set(init, s.id);
          this.callerOfNode.set(init, s.id);
        } else if (m.type && ts.isFunctionTypeNode(m.type)) {
          s.kind = "method";
          s.signature = this.signature(sf, m.type, "property", name);
          this.idOfNode.set(m.type, s.id);
        } else {
          s.type = this.typeOf(sf, m, m.type);
          if (ts.isPropertyDeclaration(m) && m.initializer) this.callerOfNode.set(m.initializer, s.id);
        }
      }
      if (mods.length) s.modifiers = mods;
      const dec = this.decorators(m);
      if (dec.length) s.annotations = dec;
      this.addSym(s, p);
      if (tsSym) seen.set(tsSym, s);
      this.idOfNode.set(m, s.id);
      if (ts.isFunctionLike(m) && "body" in m && m.body) this.callerOfNode.set(m, s.id);
      this.queueDecl(m, sf, s);
      if (ts.isConstructorDeclaration(m)) this.parameterProperties(sf, owner, ownerPath, m);
    }
  }

  /** Declares the properties that a constructor's parameters declare (constructor(private store: Store)). */
  private parameterProperties(sf: ts.SourceFile, owner: S.Symbol, ownerPath: string, ctor: ts.ConstructorDeclaration) {
    for (const prm of ctor.parameters) {
      if (!ts.isIdentifier(prm.name) || !ts.isParameterPropertyDeclaration(prm, ctor)) continue;
      const name = prm.name.text;
      const p = `${ownerPath}.${name}`;
      const mods = this.modifiers(prm).filter(x => x !== "public");
      const s = this.addSym(
        {
          id: this.symId(sf, p),
          kind: "property",
          languageKind: "property",
          name,
          parent: owner.id,
          exported: !mods.includes("private") && !mods.includes("protected"),
          location: this.loc(sf, prm, false),
          type: this.typeOf(sf, prm, prm.type),
        },
        p,
      );
      if (mods.length) s.modifiers = mods;
      this.idOfNode.set(prm, s.id);
      const tag = ts.getJSDocParameterTags(prm)[0];
      const text = tag && (typeof tag.comment === "string" ? tag.comment : ts.getTextOfJSDocComment(tag.comment));
      if (text) this.queue(text.replace(/^\s*-\s*/, ""), sf, prm, d => (s.doc = d));
    }
  }

  /* Types and signatures. */

  /** The type of a declaration: as written when it has an annotation, as the checker infers it otherwise. */
  typeOf(sf: ts.SourceFile, decl: ts.Node, node: ts.TypeNode | undefined): S.TypeText {
    if (node) return { text: collapse(node.getText(sf)), refs: this.refsLater(() => this.typeNodeRefs(node)) };
    const t = this.checker.getTypeAtLocation(decl);
    return { text: this.typeText(t, decl), refs: this.refsLater(() => this.typeRefs(t)) };
  }

  /** Returns a list that fills with refs once every symbol exists. */
  private refsLater(compute: () => string[]): string[] {
    const out: string[] = [];
    this.later.push(() => out.push(...compute()));
    return out;
  }

  /** Returns a list that fills with the IDs that an inferred type names, once every symbol exists. */
  typeRefsLater(type: ts.Type): string[] {
    return this.refsLater(() => this.typeRefs(type));
  }

  private typeText(t: ts.Type, at: ts.Node): string {
    return this.checker.typeToString(t, at, ts.TypeFormatFlags.UseAliasDefinedOutsideCurrentScope);
  }

  /** The IDs of the symbols that a written type names. */
  typeNodeRefs(node: ts.TypeNode): string[] {
    const out = new Set<string>();
    const add = (n: ts.Node) => {
      const target = ts.isQualifiedName(n) ? n.right : ts.isPropertyAccessExpression(n) ? n.name : n;
      const id = this.refTarget(this.idOfSymbol(this.checker.getSymbolAtLocation(target)));
      if (id) out.add(id);
    };
    const visit = (n: ts.Node) => {
      if (ts.isTypeReferenceNode(n)) add(n.typeName);
      else if (ts.isExpressionWithTypeArguments(n)) add(n.expression);
      else if (ts.isTypeQueryNode(n)) add(n.exprName);
      else if (ts.isImportTypeNode(n) && n.qualifier) add(n.qualifier);
      ts.forEachChild(n, visit);
    };
    visit(node);
    return [...out].sort(cmp);
  }

  /** The IDs of the symbols that an inferred type names. */
  typeRefs(type: ts.Type): string[] {
    const out = new Set<string>();
    const seen = new Set<ts.Type>();
    const walkType = (t: ts.Type, depth: number) => {
      if (depth > 6 || seen.has(t)) return;
      seen.add(t);
      if (t.aliasSymbol) {
        const id = this.refTarget(this.idOfSymbol(t.aliasSymbol));
        if (id) {
          out.add(id);
          return;
        }
        for (const a of t.aliasTypeArguments ?? []) walkType(a, depth + 1);
        return;
      }
      const sym = t.getSymbol();
      const id =
        sym &&
        sym.flags & (ts.SymbolFlags.Class | ts.SymbolFlags.Interface | ts.SymbolFlags.Enum | ts.SymbolFlags.EnumMember)
          ? this.refTarget(this.idOfSymbol(sym))
          : undefined;
      if (id) out.add(id);
      if (t.isUnionOrIntersection()) {
        if (!id) for (const u of t.types) walkType(u, depth + 1);
        return;
      }
      if (t.flags & ts.TypeFlags.Object && (t as ts.ObjectType).objectFlags & ts.ObjectFlags.Reference)
        for (const a of this.checker.getTypeArguments(t as ts.TypeReference)) walkType(a, depth + 1);
      if (!id && t.flags & ts.TypeFlags.Object)
        for (const sig of t.getCallSignatures()) {
          for (const prm of sig.parameters) walkType(this.checker.getTypeOfSymbol(prm), depth + 1);
          walkType(sig.getReturnType(), depth + 1);
        }
    };
    walkType(type, 0);
    return [...out].sort(cmp);
  }

  /** A type reference to an enum member counts as a reference to its enum. */
  private refTarget(id: string | undefined): string | undefined {
    const s = id ? this.syms.get(id) : undefined;
    if (s && s.languageKind === "enum member") return s.parent;
    return id;
  }

  /** Builds the signature of a callable: its parameters and results with their types, and the text as the code writes it. `how` names the form ("function", "arrow", "constructor", "property" or "method"). */
  signature(sf: ts.SourceFile, fn: ts.SignatureDeclaration, how: string, name: string): S.Signature {
    const params: S.Param[] = [];
    for (const prm of fn.parameters) {
      const pn = ts.isIdentifier(prm.name) ? prm.name.text : shorten(collapse(prm.name.getText(sf)), 40);
      if (pn === "this") continue;
      const t = this.typeOf(sf, prm, prm.type);
      const p: S.Param = { name: pn, type: (prm.dotDotDotToken ? "..." : "") + t.text, refs: t.refs };
      if (prm.questionToken || prm.initializer) p.optional = true;
      if (prm.dotDotDotToken) p.rest = true;
      if (prm.initializer) p.default = shorten(collapse(prm.initializer.getText(sf)), 40);
      params.push(p);
    }
    const results: S.Param[] = [];
    if (!ts.isConstructorDeclaration(fn) && !ts.isSetAccessor(fn)) {
      if (fn.type) {
        if (fn.type.kind !== ts.SyntaxKind.VoidKeyword) {
          const node = fn.type;
          results.push({ type: collapse(node.getText(sf)), refs: this.refsLater(() => this.typeNodeRefs(node)) });
        }
      } else {
        const sig = this.checker.getSignatureFromDeclaration(fn);
        const rt = sig && this.checker.getReturnTypeOfSignature(sig);
        if (rt && !(rt.flags & ts.TypeFlags.Void)) {
          results.push({ type: this.typeText(rt, fn), refs: this.refsLater(() => this.typeRefs(rt)) });
        }
      }
    }
    const s: S.Signature = { params, results };
    const tps = fn.typeParameters?.length
      ? "<" + fn.typeParameters.map(t => collapse(t.getText(sf))).join(", ") + ">"
      : "";
    if (tps) s.typeParams = tps;
    const ps = fn.parameters.map(p => collapse(p.getText(sf))).join(", ");
    const ret = fn.type ? ": " + collapse(fn.type.getText(sf)) : "";
    const isAsync = this.modifiers(fn).includes("async") ? "async " : "";
    const star =
      (ts.isFunctionDeclaration(fn) || ts.isFunctionExpression(fn) || ts.isMethodDeclaration(fn)) && fn.asteriskToken
        ? "*"
        : "";
    switch (how) {
      case "function":
        s.text = `${isAsync}function${star} ${name}${tps}(${ps})${ret}`;
        break;
      case "arrow":
        s.text = `const ${name} = ${isAsync}${tps}(${ps})${ret} => …`;
        break;
      case "constructor":
        s.text = `constructor(${ps})`;
        break;
      case "property":
        s.text = ts.isFunctionTypeNode(fn)
          ? `${name}: ${collapse(fn.getText(sf))}`
          : `${name} = ${isAsync}${tps}(${ps})${ret} => …`;
        break;
      default: {
        const mods = this.modifiers(fn).filter(m => ["static", "abstract", "async"].includes(m));
        s.text = `${mods.map(m => m + " ").join("")}${star}${name}${tps}(${ps})${ret}`;
      }
    }
    return s;
  }

  /** Declares an inline route handler at the top level of a module as a function named after its route. */
  inlineHandler(fn: ts.ArrowFunction | ts.FunctionExpression | ts.MethodDeclaration, name: string): string {
    const known = this.idOfNode.get(fn);
    if (known) return known;
    const sf = fn.getSourceFile();
    const s = this.addSym(
      {
        id: this.symId(sf, name),
        kind: "function",
        languageKind: ts.isArrowFunction(fn) ? "arrow function" : "function",
        name,
        parent: this.fileId(sf),
        exported: false,
        location: this.loc(sf, fn),
        signature: this.signature(sf, fn, ts.isArrowFunction(fn) ? "arrow" : "function", "handler"),
      },
      name,
    );
    s.signature!.text = s.signature!.text!.replace(/^const handler = /, "");
    this.idOfNode.set(fn, s.id);
    this.callerOfNode.set(fn, s.id);
    return s.id;
  }

  /* Pass 3: relations. */

  private relations() {
    for (const f of this.later.splice(0)) f();
    for (const sf of this.files) {
      const visit = (n: ts.Node) => {
        if (ts.isClassDeclaration(n) || ts.isInterfaceDeclaration(n)) this.heritage(n);
        if (ts.isModuleDeclaration(n) || ts.isModuleBlock(n) || ts.isSourceFile(n)) ts.forEachChild(n, visit);
      };
      visit(sf);
    }
    for (const s of [...this.syms.values()].sort((a, b) => cmp(a.id, b.id))) {
      if (s.kind === "property" && s.type)
        for (const r of s.type.refs ?? []) this.addEdge(s.id, r, "field_type", s.location.line);
      if (s.signature) {
        for (const p of s.signature.params)
          for (const r of p.refs ?? []) this.addEdge(s.id, r, "param_type", s.location.line);
        for (const p of s.signature.results)
          for (const r of p.refs ?? []) this.addEdge(s.id, r, "return_type", s.location.line);
      }
    }
  }

  /** Records extends and implements clauses, and the methods that override a base class's methods. */
  private heritage(node: ts.ClassDeclaration | ts.InterfaceDeclaration) {
    const id = this.idOfNode.get(node);
    if (!id) return;
    for (const clause of node.heritageClauses ?? []) {
      for (const t of clause.types) {
        const target = this.idOfSymbol(this.checker.getSymbolAtLocation(t.expression));
        if (!target) continue;
        const kind = clause.token === ts.SyntaxKind.ImplementsKeyword ? "implements" : "extends";
        this.addEdge(id, target, kind, 0);
      }
    }
    if (!ts.isClassDeclaration(node)) return;
    const type = this.checker.getTypeAtLocation(node);
    const bases = type.isClassOrInterface() ? this.checker.getBaseTypes(type) : [];
    if (!bases.length) return;
    for (const m of node.members) {
      if (
        !(ts.isMethodDeclaration(m) || ts.isGetAccessor(m) || ts.isSetAccessor(m) || ts.isPropertyDeclaration(m)) ||
        !m.name
      )
        continue;
      const mid = this.idOfNode.get(m);
      if (!this.isCallable(mid) || ts.getCombinedModifierFlags(m) & ts.ModifierFlags.Static) continue;
      const name = this.memberName(m, node.getSourceFile());
      for (const b of bases) {
        const prop = name && this.checker.getPropertyOfType(b, name);
        const target = prop && this.idOfSymbol(prop);
        if (target && target !== mid) {
          this.addEdge(mid!, target, "overrides", 0);
          break;
        }
      }
    }
  }

  /** Records which folders a file's folder imports, and the outside packages it imports. */
  private imports(sf: ts.SourceFile) {
    const dir = this.dirIdOf(sf);
    const c = this.conts.get(dir)!;
    const ext = new Set(c.externalImports ?? []);
    const specs: ts.StringLiteralLike[] = [];
    for (const st of sf.statements) {
      if (
        (ts.isImportDeclaration(st) || ts.isExportDeclaration(st)) &&
        st.moduleSpecifier &&
        ts.isStringLiteralLike(st.moduleSpecifier)
      )
        specs.push(st.moduleSpecifier);
      else if (
        ts.isImportEqualsDeclaration(st) &&
        ts.isExternalModuleReference(st.moduleReference) &&
        ts.isStringLiteralLike(st.moduleReference.expression)
      )
        specs.push(st.moduleReference.expression);
    }
    for (const spec of specs) {
      const sym = this.checker.getSymbolAtLocation(spec);
      const target = sym?.declarations?.find(ts.isSourceFile);
      if (target && this.mapped.has(target.fileName)) {
        this.addEdge(dir, this.dirIdOf(target), "imports", 0);
        continue;
      }
      const text = spec.text;
      if (this.svelte && !target && /(^|\/)\$types(\.[jt]s)?$/.test(text)) this.svelte.unresolvedTypes++;
      if (text.startsWith(".") || text.startsWith("/")) continue;
      // $lib and #-imports name the project's own files, such as an asset
      // that only the bundler reads; they never name an outside package.
      if (this.svelte && /^(\$lib(\/|$)|#)/.test(text)) continue;
      const parts = text.split("/");
      const name = text.startsWith("@") ? parts.slice(0, 2).join("/") : parts[0];
      if (name.startsWith("node:") || BUILTIN.has(name) || name === this.pkgName) continue;
      // In a project with components, an import that an ambient module
      // declaration answers (svelte's own types declare "svelte") resolves.
      if (!target && !(this.svelte && sym)) this.unresolved++;
      ext.add(name);
    }
    if (ext.size) c.externalImports = [...ext].sort(cmp);
  }

  /** The local names that a file's imports bind. A call through one may reach the project's code. */
  private aliases(sf: ts.SourceFile): Set<string> {
    let set = this.aliasesOf.get(sf);
    if (set) return set;
    set = new Set();
    for (const st of sf.statements) {
      if (!ts.isImportDeclaration(st) || !st.importClause) continue;
      const ic = st.importClause;
      if (ic.name) set.add(ic.name.text);
      const nb = ic.namedBindings;
      if (nb && ts.isNamespaceImport(nb)) set.add(nb.name.text);
      if (nb && ts.isNamedImports(nb)) for (const e of nb.elements) set.add(e.name.text);
    }
    this.aliasesOf.set(sf, set);
    return set;
  }

  /** Walks a file and calls fn for every node with the ID of the symbol whose code holds it. */
  walkWithCaller(sf: ts.SourceFile, fn: (n: ts.Node, caller: string | undefined) => void) {
    const visit = (n: ts.Node, caller: string | undefined) => {
      caller = this.callerOfNode.get(n) ?? caller;
      fn(n, caller);
      ts.forEachChild(n, c => visit(c, caller));
    };
    visit(sf, undefined);
  }

  private walkCalls(sf: ts.SourceFile) {
    const aliases = this.aliases(sf);
    const component = this.svelte?.has(sf.fileName);
    this.walkWithCaller(sf, (n, caller) => {
      if (!caller) return;
      if (component) {
        // A component tag renders that component: <Button /> calls Button.
        const r = this.svelte!.renderTarget(this, n);
        if (r) {
          this.addEdge(caller, r.id, "calls", this.line(r.at, sf));
          return;
        }
      }
      if (!(
        ts.isCallExpression(n) ||
        ts.isNewExpression(n) ||
        ts.isTaggedTemplateExpression(n) ||
        ts.isDecorator(n) ||
        ts.isJsxOpeningLikeElement(n)
      ))
        return;
      const name = calleeName(n);
      if (name !== "super" && (!name || (!this.names.has(name) && !aliases.has(name)))) return;
      const sig = this.checker.getResolvedSignature(n);
      const decl = sig?.declaration;
      const callee = decl && this.idOfNode.get(decl);
      if (callee && this.syms.has(callee)) this.addEdge(caller, callee, "calls", this.line(n, sf));
    });
  }

  /* Pass 4: docs. */

  /** Queues the doc comment above a declaration as the doc of its symbol. A symbol takes the first doc that it meets, so an overload's second comment never wins. */
  queueDecl(node: ts.Node, sf: ts.SourceFile, s: S.Symbol) {
    if (this.docQueued.has(s.id)) return;
    const jsdocs = ts.getJSDocCommentsAndTags(node).filter(ts.isJSDoc);
    const last = jsdocs[jsdocs.length - 1];
    if (!last) return;
    const raw = commentText(sf.text.slice(last.pos, last.end));
    if (!raw || FILE_TAGS.test(raw)) return;
    this.docQueued.add(s.id);
    this.queue(raw, sf, node, d => (s.doc = d));
  }

  /** Queues a doc comment, which pass 4 turns into the normal form once every symbol exists, so that its links resolve. */
  queue(raw: string, sf: ts.SourceFile, at: ts.Node, set: (d: S.Doc) => void) {
    this.docs.push({ raw, sf, at, set });
  }

  /** Queues the doc of one parameter, such as a component's prop: its text joins the owner's doc under the parameter's name. */
  queueParamDoc(raw: string, sf: ts.SourceFile, at: ts.Node, owner: S.Symbol, name: string) {
    this.docs.push({
      raw,
      sf,
      at,
      set: () => {},
      inline: text => {
        owner.doc ??= { raw: "", format: "jsdoc", summary: "", description: [] };
        (owner.doc.params ??= {})[name] = text;
      },
    });
  }

  private resolveDocs() {
    for (const pd of this.docs) {
      const lang = /\.[cm]?jsx?$/.test(pd.sf.fileName) ? "js" : "ts";
      const resolve = (t: string) => this.resolveLink(t, pd.sf, pd.at);
      if (pd.inline) {
        const d = normalise(pd.raw, resolve, lang);
        const paras = d?.description.flatMap(b => (b.type === "paragraph" ? [b.text] : [])) ?? [];
        const text = paras.length ? paras.join(" ") : (d?.summary ?? inline(pd.raw, resolve));
        if (text) pd.inline(text);
        continue;
      }
      const d = normalise(pd.raw, resolve, lang);
      if (d) pd.set(d);
    }
  }

  /** Resolves the target of a {@link} or @see: a name in scope, then members by "." or "#". */
  private resolveLink(target: string, sf: ts.SourceFile, at: ts.Node): string | undefined {
    const parts = target.split(/[.#]/).filter(Boolean);
    if (!parts.length) return undefined;
    // A sibling member, for links inside a member's doc.
    const self = this.idOfNode.get(at);
    const owner = self && this.syms.get(self)?.parent;
    const ownerPath = owner && this.pathOf.get(owner);
    if (ownerPath) {
      const sib = this.syms.get(this.symId(sf, `${ownerPath}.${parts.join(".")}`));
      if (sib) return "sym:" + sib.id;
    }
    let sym: ts.Symbol | undefined = this.scope(sf).get(parts[0]);
    for (const part of parts.slice(1)) {
      if (!sym) break;
      if (sym.flags & ts.SymbolFlags.Alias) {
        try {
          sym = this.checker.getAliasedSymbol(sym);
        } catch {
          return undefined;
        }
      }
      const key = ts.escapeLeadingUnderscores(part);
      sym =
        sym.exports?.get(key) ??
        sym.members?.get(key) ??
        this.checker.getPropertyOfType(this.checker.getDeclaredTypeOfSymbol(sym), part) ??
        this.checker.getPropertyOfType(this.checker.getTypeOfSymbol(sym), part);
    }
    const id = this.idOfSymbol(sym);
    if (id) return "sym:" + id;
    const local = this.syms.get(this.symId(sf, parts.join(".")));
    return local ? "sym:" + local.id : undefined;
  }

  private scope(sf: ts.SourceFile): Map<string, ts.Symbol> {
    let m = this.scopeOf.get(sf);
    if (m) return m;
    m = new Map();
    const at = sf.statements[0] ?? sf;
    const flags = ts.SymbolFlags.Type | ts.SymbolFlags.Value | ts.SymbolFlags.Namespace | ts.SymbolFlags.Alias;
    for (const s of this.checker.getSymbolsInScope(at, flags)) if (!m.has(s.name)) m.set(s.name, s);
    this.scopeOf.set(sf, m);
    return m;
  }

  /** Records that a test uses a symbol. */
  addTestUse(id: string, use: S.TestUse) {
    let m = this.testUses.get(id);
    if (!m) this.testUses.set(id, (m = new Map()));
    m.set(`${use.location.path}:${use.location.line}:${use.name}`, use);
  }

  /* Output. */

  private document(): S.Document {
    const notes = [
      "Calls through function values, callbacks and computed property names do not appear.",
      "A call through an interface points at the interface member.",
      "Declaration files (.d.ts) do not appear; they describe code that lives elsewhere.",
    ];
    if (this.unresolved)
      notes.push(
        `${this.unresolved} imports of outside packages did not resolve, so the types they bring count as unknown.`,
      );
    if (this.svelte) notes.push(...this.svelte.limits());
    notes.push(...this.notes);
    const info: S.AdapterInfo = { name: "typescript", version: ADAPTER_VERSION, language: "ts" };
    const r = this.rel(this.root);
    if (r !== ".") info.root = r;
    info.notes = notes;
    const d: S.Document = {
      version: VERSION,
      meta: {
        schemaVersion: VERSION,
        tool: { name: "acu-adapter-typescript", version: ADAPTER_VERSION },
        generated: "",
        languages: ["ts"],
        adapters: [info],
      },
      containers: [...this.conts.values()].sort((a, b) => cmp(a.id, b.id)),
      symbols: [],
      edges: [],
    };
    for (const s of [...this.syms.values()].sort((a, b) => cmp(a.id, b.id))) {
      const keep = (refs?: string[]) => (refs ?? []).filter(x => this.syms.has(x));
      const tidy = (o: { refs?: string[] }) => {
        const k = keep(o.refs);
        if (k.length) o.refs = k;
        else delete o.refs;
      };
      if (s.type) tidy(s.type);
      if (s.signature) for (const p of [...s.signature.params, ...s.signature.results]) tidy(p);
      d.symbols.push(s);
    }
    const known = (id: string) => this.syms.has(id) || this.conts.has(id);
    for (const e of this.edges.values()) {
      if (!known(e.from) || !known(e.to)) continue;
      if (e.count === 1) delete e.count;
      d.edges.push(e);
    }
    d.edges.sort((a, b) => cmp(a.from, b.from) || cmp(a.to, b.to) || cmp(a.kind, b.kind));
    if (this.entries.length) {
      const list = [...this.entries].sort(
        (a, b) =>
          cmp(a.registeredAt.path, b.registeredAt.path) || a.registeredAt.line - b.registeredAt.line || cmp(a.id, b.id),
      );
      const seen = new Set<string>();
      for (const e of list) {
        if (seen.has(e.id)) e.id = `${e.id}@${path.posix.basename(e.registeredAt.path)}:${e.registeredAt.line}`;
        seen.add(e.id);
      }
      d.entryPoints = list;
    }
    if (this.testUses.size) {
      d.tests = {};
      for (const k of [...this.testUses.keys()].sort(cmp)) {
        d.tests[k] = [...this.testUses.get(k)!.values()].sort(
          (a, b) => cmp(a.location.path, b.location.path) || a.location.line - b.location.line || cmp(a.name, b.name),
        );
      }
    }
    return d;
  }
}

/** Compares two strings by code unit, so the order does not depend on the locale. */
export function cmp(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/** The name that a call reaches for: f in f(), get in a.get(), "super" in super(), or a JSX tag's name. */
function calleeName(n: ts.Node): string | undefined {
  let e: ts.Node | undefined;
  if (ts.isCallExpression(n) || ts.isNewExpression(n) || ts.isDecorator(n)) e = n.expression;
  else if (ts.isTaggedTemplateExpression(n)) e = n.tag;
  else if (ts.isJsxOpeningLikeElement(n)) e = n.tagName;
  while (e && ts.isCallExpression(e)) e = e.expression;
  if (!e) return undefined;
  if (e.kind === ts.SyntaxKind.SuperKeyword) return "super";
  if (ts.isIdentifier(e)) return e.text;
  if (ts.isPropertyAccessExpression(e)) return e.name.text;
  if (ts.isElementAccessExpression(e) && ts.isStringLiteralLike(e.argumentExpression)) return e.argumentExpression.text;
  return undefined;
}

/** Reads and parses a tsconfig file, with its extends chain, or throws with a message that names the file. `extra` overrides the file's own options. */
function parseConfig(file: string, extra?: ts.CompilerOptions): ts.ParsedCommandLine {
  const host: ts.ParseConfigFileHost = {
    ...ts.sys,
    onUnRecoverableConfigFileDiagnostic: d => {
      throw new Error(`read ${file}: ${ts.flattenDiagnosticMessageText(d.messageText, " ")}`);
    },
  };
  const parsed = ts.getParsedCommandLineOfConfigFile(file, extra, host);
  if (!parsed) throw new Error(`read ${file}: the file does not parse`);
  return parsed;
}

/** Returns the name in a folder's package.json, or undefined when it has none. */
function readPackageName(root: string): string | undefined {
  try {
    const p = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8")) as { name?: unknown };
    return typeof p.name === "string" && p.name ? p.name : undefined;
  } catch {
    return undefined;
  }
}

/** Lists the files under a folder. It leaves out node_modules, folders whose names start with a dot, and folders that `skip` names by their path from `dir`. */
function walk(dir: string, skip: (rel: string) => boolean = () => false): string[] {
  const out: string[] = [];
  const visit = (d: string) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      if (e.name.startsWith(".") || e.name === "node_modules") continue;
      const p = path.join(d, e.name);
      if (e.isDirectory()) {
        if (!skip(path.relative(dir, p).split(path.sep).join("/"))) visit(p);
      } else if (e.isFile()) out.push(p);
    }
  };
  visit(dir);
  return out.sort();
}

/** Reads the project that a request names and returns its document. */
export function run(req: Request): S.Document {
  return new Extractor(req).run();
}
