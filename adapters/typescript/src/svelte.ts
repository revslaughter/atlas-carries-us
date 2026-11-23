// Svelte components in the TypeScript program. svelte2tsx turns each .svelte
// file into TypeScript. The adapter serves that code to the compiler under a
// virtual name that ends in ".acu.tsx" (or ".acu.jsx" for a component in
// JavaScript), resolves imports of the component to it, and maps every
// location back to the .svelte file through the source map that svelte2tsx
// writes. This file also declares each component's symbols: the component
// itself with its props, its snippets and events, and the declarations of
// its scripts. The adapter turns all of this on only when the root holds a
// .svelte file, so a project without one reads exactly as before.

import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import ts from "typescript";

import { commentText } from "./docs.js";
import { collapse, type Extractor, unwrap } from "./extract.js";
import type * as S from "./schema.js";
import { SourceMap } from "./sourcemap.js";

/** Loads the Svelte tools from the adapter's own install, on the first run that needs them. */
const require = createRequire(import.meta.url);

/** The name that svelte2tsx gives a call that makes one component render another: `<Button />` becomes `__sveltets_2_ensureComponent(Button)`. */
const RENDER_CALL = "__sveltets_2_ensureComponent";
/** The end that TypeScript's module resolution probes for an import with an unknown extension: "./Button.svelte" makes it look for "./Button.d.svelte.ts". */
const PROBE = ".d.svelte.ts";
/** The svelte2tsx files that declare the helpers its output calls, and the elements of the markup. */
const SHIMS = ["svelte2tsx/svelte-shims-v4.d.ts", "svelte2tsx/svelte-jsx-v4.d.ts"];

/** A range of offsets in a component's source: from `start` up to, but not including, `end`. */
interface Range {
  /** The first offset. */
  start: number;
  /** The offset after the last one. */
  end: number;
}

/** One component: its source, the TypeScript that svelte2tsx writes for it, and what the parse found. */
export interface Component {
  /** The absolute path of the .svelte file. */
  file: string;
  /** The absolute virtual name under which the compiler reads the TypeScript. */
  virtual: string;
  /** TSX for a component in TypeScript, JSX for one in JavaScript, so that JSDoc types count. */
  kind: ts.ScriptKind;
  /** The extension of the virtual name, for module resolution. */
  extension: ts.Extension;
  /** The component's source. */
  source: string;
  /** The offset at which each source line starts. */
  lineStarts: number[];
  /** The TypeScript that svelte2tsx writes. */
  code: string;
  /** Maps the TypeScript back to the source. */
  map: SourceMap;
  /** The content of the instance script, if any. */
  instance?: Range;
  /** The content of the module script, if any. */
  module?: Range;
  /** The snippets of the markup, by name, in source order. */
  snippets: (Range & { name: string })[];
  /** The events that the component dispatches or forwards, with their types and docs. */
  events: { name: string; type: string; doc?: string }[];
  /** The component's ID, once {@link Svelte.declare} has made it. */
  id?: string;
}

/** The part of a node of the Svelte syntax tree that the adapter reads. */
interface SvelteNode {
  /** The node's type, such as "SnippetBlock". */
  type?: string;
  /** The offset of the node's first character. */
  start?: number;
  /** The offset after the node's last character. */
  end?: number;
  /** A snippet's name. */
  expression?: { name?: string };
}

/** The part of a script of the Svelte syntax tree that the adapter reads. */
interface SvelteScript {
  /** The script's code, without its tags. */
  content: { start: number; end: number };
}

/** Holds the components of one run and everything that reads them. */
export class Svelte {
  /** The components, by virtual name. */
  private readonly byVirtual = new Map<string, Component>();
  /** The components, by the path of their .svelte file. */
  private readonly byFile = new Map<string, Component>();
  /** The limits of this run that the components add, for the output's notes. */
  readonly notes: string[] = [];
  /** How many imports of SvelteKit's generated route types (./$types) did not resolve. */
  unresolvedTypes = 0;
  /** The version of the Svelte compiler that parses components. */
  readonly svelteVersion: string;

  /**
   * Turns each .svelte file into TypeScript. A file that does not parse adds
   * a note and stays out of the map; the others carry on.
   */
  constructor(files: readonly string[], rel: (file: string) => string) {
    const compiler = require("svelte/compiler") as typeof import("svelte/compiler");
    const { svelte2tsx } = require("svelte2tsx") as typeof import("svelte2tsx");
    this.svelteVersion = compiler.VERSION;
    for (const file of files) {
      const source = fs.readFileSync(file, "utf8");
      try {
        const ast = compiler.parse(source, { modern: true }) as unknown as {
          instance?: SvelteScript & { attributes: { name: string; value: unknown }[] };
          module?: SvelteScript & { attributes: { name: string; value: unknown }[] };
          fragment: unknown;
        };
        const isTs = [ast.instance, ast.module].some(s =>
          s?.attributes.some(a => a.name === "lang" && langIsTs(a.value)),
        );
        const out = svelte2tsx(source, {
          filename: file,
          isTsFile: isTs,
          mode: "ts",
          emitOnTemplateError: true,
          version: compiler.VERSION,
        });
        const c: Component = {
          file,
          virtual: file + (isTs ? ".acu.tsx" : ".acu.jsx"),
          kind: isTs ? ts.ScriptKind.TSX : ts.ScriptKind.JSX,
          extension: isTs ? ts.Extension.Tsx : ts.Extension.Jsx,
          source,
          lineStarts: lineStarts(source),
          code: out.code,
          map: new SourceMap(out.map.mappings),
          snippets: snippets(ast.fragment),
          events: out.events.getAll(),
        };
        if (ast.instance) c.instance = { start: ast.instance.content.start, end: ast.instance.content.end };
        if (ast.module) c.module = { start: ast.module.content.start, end: ast.module.content.end };
        this.byVirtual.set(c.virtual, c);
        this.byFile.set(file, c);
      } catch (e) {
        const msg = (e instanceof Error ? e.message : String(e)).split("\n")[0];
        this.notes.push(
          `${rel(file)} does not parse with svelte ${compiler.VERSION}, so the map leaves it out: ${msg.replace(/\.$/, "")}.`,
        );
      }
    }
  }

  /** The virtual names of the components, in the order of their files. */
  virtualNames(): string[] {
    return [...this.byVirtual.keys()];
  }

  /** The files that declare the helpers and elements that svelte2tsx's output names, from the adapter's own install. */
  shims(): string[] {
    return SHIMS.map(s => require.resolve(s));
  }

  /** Reports whether a file name is the virtual name of a component. */
  has(fileName: string): boolean {
    return this.byVirtual.has(fileName);
  }

  /** The component that a virtual name or a .svelte path names, if any. */
  get(fileName: string): Component | undefined {
    return this.byVirtual.get(fileName) ?? this.byFile.get(fileName);
  }

  /** The .svelte path behind a virtual name; any other file name stays as it is. */
  real(fileName: string): string {
    return this.byVirtual.get(fileName)?.file ?? fileName;
  }

  /**
   * Applies the compiler options that svelte-check forces: an ES module kind,
   * a module resolution, and a target of at least ES2015. Where the config
   * names no module resolution, the adapter picks the one that SvelteKit's
   * generated config sets (Bundler), not svelte-check's Node10, because only
   * the newer ones read the `imports` of package.json, where SvelteKit 3
   * declares `#lib`. It also maps `$lib` to `src/lib`, as SvelteKit 2 does,
   * when the config gives no paths for it.
   */
  configure(options: ts.CompilerOptions, root: string) {
    const M = ts.ModuleKind;
    if (options.module === undefined || [M.AMD, M.CommonJS, M.ES2015, M.None, M.System, M.UMD].includes(options.module))
      options.module = M.ESNext;
    if (options.moduleResolution === undefined || options.moduleResolution === ts.ModuleResolutionKind.Classic) {
      const node = [M.Node16, M.Node18, M.Node20, M.NodeNext].includes(options.module);
      options.moduleResolution = node ? ts.ModuleResolutionKind.NodeNext : ts.ModuleResolutionKind.Bundler;
    }
    if (options.target === undefined) options.target = ts.ScriptTarget.Latest;
    else if (options.target < ts.ScriptTarget.ES2015) options.target = ts.ScriptTarget.ES2015;
    if (!options.paths?.["$lib"] && !options.paths?.["$lib/*"]) {
      const lib = path.join(root, "src", "lib");
      options.paths = { ...options.paths, $lib: [lib], "$lib/*": [path.join(lib, "*")] };
      if (options.baseUrl === undefined && options.pathsBasePath === undefined) options.pathsBasePath = root;
    }
  }

  /**
   * Wraps a compiler host so that the compiler reads each component's
   * TypeScript under its virtual name, and resolves an import of a .svelte
   * file to it. The resolution runs TypeScript's own, so paths, package
   * imports and every resolution mode behave as usual: the wrapper only
   * answers yes when the resolver asks for "Button.d.svelte.ts" next to a
   * known "Button.svelte", and then hands back the virtual name.
   */
  wrap(host: ts.CompilerHost, options: ts.CompilerOptions): ts.CompilerHost {
    const cache = ts.createModuleResolutionCache(
      host.getCurrentDirectory(),
      f => host.getCanonicalFileName(f),
      options,
    );
    const probe = (f: string) => f.endsWith(PROBE) && this.byFile.has(f.slice(0, -PROBE.length) + ".svelte");
    const resolution: ts.ModuleResolutionHost = {
      fileExists: f => probe(f) || host.fileExists(f),
      readFile: f => host.readFile(f),
      directoryExists: host.directoryExists && (d => host.directoryExists!(d)),
      realpath: host.realpath && (f => host.realpath!(f)),
      getCurrentDirectory: () => host.getCurrentDirectory(),
      getDirectories: host.getDirectories && (d => host.getDirectories!(d)),
      useCaseSensitiveFileNames: host.useCaseSensitiveFileNames(),
    };
    return {
      ...host,
      fileExists: f => this.byVirtual.has(f) || host.fileExists(f),
      readFile: f => this.byVirtual.get(f)?.code ?? host.readFile(f),
      getSourceFile: (f, lang, onError, create) => {
        const c = this.byVirtual.get(f);
        return c ? ts.createSourceFile(f, c.code, lang, true, c.kind) : host.getSourceFile(f, lang, onError, create);
      },
      resolveModuleNameLiterals: (literals, containing, redirected, opts, containingFile) =>
        literals.map(lit => {
          const mode = ts.getModeForUsageLocation(containingFile, lit, opts);
          const r = ts.resolveModuleName(lit.text, containing, opts, resolution, cache, redirected, mode);
          const f = r.resolvedModule?.resolvedFileName;
          const c = f?.endsWith(PROBE) ? this.byFile.get(f.slice(0, -PROBE.length) + ".svelte") : undefined;
          if (!c) return r;
          return {
            resolvedModule: {
              resolvedFileName: c.virtual,
              extension: c.extension,
              isExternalLibraryImport: false,
              resolvedUsingTsExtension: false,
            },
          };
        }),
    };
  }

  /** The line in the .svelte file, counting from 1, of a position in a component's TypeScript; undefined for any other file. A position without a mapping takes the component's first line. */
  lineOf(sf: ts.SourceFile, pos: number): number | undefined {
    const c = this.byVirtual.get(sf.fileName);
    if (!c) return undefined;
    const lc = sf.getLineAndCharacterOfPosition(pos);
    const o = c.map.original(lc.line, lc.character);
    return o ? o.line + 1 : 1;
  }

  /** The offset in the .svelte source of a position in a component's TypeScript, or undefined when it has no mapping. */
  private offsetOf(c: Component, sf: ts.SourceFile, pos: number): number | undefined {
    const lc = sf.getLineAndCharacterOfPosition(pos);
    const o = c.map.original(lc.line, lc.character);
    return o && (c.lineStarts[o.line] ?? c.source.length) + o.col;
  }

  /** Reports which script of the source a generated node comes from: "module", "instance", or undefined for the markup and the code that svelte2tsx adds. */
  private origin(c: Component, sf: ts.SourceFile, node: ts.Node): "module" | "instance" | undefined {
    const at = this.offsetOf(c, sf, node.getStart(sf));
    if (at === undefined) return undefined;
    if (c.module && at >= c.module.start && at < c.module.end) return "module";
    if (c.instance && at >= c.instance.start && at < c.instance.end) return "instance";
    return undefined;
  }

  /** The line, counting from 1, of an offset in a component's source. */
  private sourceLine(c: Component, offset: number): number {
    let lo = 0;
    let hi = c.lineStarts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (c.lineStarts[mid] <= offset) lo = mid;
      else hi = mid - 1;
    }
    return lo + 1;
  }

  /**
   * Declares the symbols of one component (see docs/adapters.md, "Svelte and
   * SvelteKit"): the component as a function, with its props as parameters;
   * the declarations of its scripts, its snippets and its events as members.
   * The calls of its markup count for the component.
   */
  declare(ex: Extractor, sf: ts.SourceFile, fileId: string) {
    const c = this.byVirtual.get(sf.fileName)!;
    let render: ts.FunctionDeclaration | undefined;
    let genName = "";
    for (const st of sf.statements) {
      if (ts.isFunctionDeclaration(st) && st.name?.text === "$$render") render = st;
      if (ts.isExportAssignment(st) && ts.isIdentifier(st.expression)) genName = st.expression.text;
    }
    const name = genName.replace(/__SvelteComponent_$/, "") || path.basename(c.file, ".svelte");
    const id = ex.symId(sf, name);
    const props = render ? this.props(ex, sf, render) : [];
    const sig: S.Signature = {
      params: props.map(p => p.param),
      results: [],
      text: `<${name}${props.map(p => ` {${p.param.name}}`).join("")} />`,
    };
    if (render?.typeParameters?.length)
      sig.typeParams = "<" + render.typeParameters.map(t => collapse(t.getText(sf))).join(", ") + ">";
    const comp = ex.addSym(
      {
        id,
        kind: "function",
        languageKind: "component",
        name,
        parent: fileId,
        exported: true,
        location: { path: ex.rel(sf.fileName), line: 1, endLine: this.sourceLine(c, Math.max(0, c.source.length - 1)) },
        signature: sig,
      },
      name,
    );
    c.id = comp.id;
    // The generated constant and type carry the component's name for every
    // import of it, and its doc: svelte2tsx turns the `@component` comment
    // into JSDoc above the constant.
    for (const st of sf.statements) {
      if (ts.isVariableStatement(st))
        for (const d of st.declarationList.declarations)
          if (ts.isIdentifier(d.name) && d.name.text === genName) {
            ex.idOfNode.set(d, comp.id);
            ex.queueDecl(d, sf, comp);
          }
      if (ts.isTypeAliasDeclaration(st) && st.name.text === genName) ex.idOfNode.set(st, comp.id);
    }
    for (const p of props) if (p.doc) ex.queueParamDoc(p.doc.raw, p.doc.sf, p.doc.at, comp, p.param.name!);
    if (render) ex.callerOfNode.set(render, comp.id);

    const propNames = new Set(props.map(p => p.param.name));
    const own = (st: ts.Statement) =>
      !isGenerated(st, genName) && !isPropVariable(st, propNames) && !ts.isExportAssignment(st);
    const top = sf.statements.filter(st => st !== render && own(st));
    const body = render?.body?.statements.filter(own) ?? [];
    const prefix = name + ".";
    ex.statements(
      sf,
      top.filter(st => this.origin(c, sf, st) === "module"),
      comp.id,
      prefix,
      true,
    );
    ex.statements(
      sf,
      [...top, ...body].filter(st => this.origin(c, sf, st) === "instance"),
      comp.id,
      prefix,
      false,
    );
    if (render) this.declareSnippets(ex, sf, c, render, comp, prefix);
    if (render) this.declareEvents(ex, sf, c, render, comp, prefix);
  }

  /** Reads a component's props: from the type of what `$$render` returns as `props`, which svelte2tsx builds from `$props()` or from `export let`. */
  private props(
    ex: Extractor,
    sf: ts.SourceFile,
    render: ts.FunctionDeclaration,
  ): { param: S.Param; doc?: { raw: string; sf: ts.SourceFile; at: ts.Node } }[] {
    const ret = render.body?.statements.find(ts.isReturnStatement)?.expression;
    const obj = ret && unwrap(ret);
    if (!obj || !ts.isObjectLiteralExpression(obj)) return [];
    const prop = obj.properties.find(
      p => ts.isPropertyAssignment(p) && ts.isIdentifier(p.name) && p.name.text === "props",
    ) as ts.PropertyAssignment | undefined;
    if (!prop) return [];
    const type = ex.checker.getTypeAtLocation(prop.initializer);
    const defaults = propDefaults(render);
    const out: { param: S.Param; doc?: { raw: string; sf: ts.SourceFile; at: ts.Node } }[] = [];
    for (const p of ex.checker.getPropertiesOfType(type)) {
      if (p.name.startsWith("$$")) continue;
      const decl = p.valueDeclaration ?? p.declarations?.[0];
      const dsf = decl?.getSourceFile();
      const written =
        decl && dsf && (ts.isPropertySignature(decl) || ts.isPropertyDeclaration(decl)) && decl.type
          ? decl.type
          : undefined;
      let t: S.TypeText;
      if (written && dsf && !ts.isTypeQueryNode(written)) t = ex.typeOf(dsf, decl!, written);
      else {
        const pt = ex.checker.getNonNullableType(ex.checker.getTypeOfSymbolAtLocation(p, prop.initializer));
        t = {
          text: ex.checker.typeToString(pt, render, ts.TypeFormatFlags.UseAliasDefinedOutsideCurrentScope),
          refs: ex.typeRefsLater(pt),
        };
      }
      const param: S.Param = { name: p.name, type: t.text, refs: t.refs };
      const def = defaults.get(p.name);
      if (p.flags & ts.SymbolFlags.Optional || def !== undefined) param.optional = true;
      if (def !== undefined) param.default = def;
      const item: { param: S.Param; doc?: { raw: string; sf: ts.SourceFile; at: ts.Node } } = { param };
      const raw = decl && dsf ? lastDoc(decl, dsf) : undefined;
      if (raw) item.doc = { raw, sf: dsf!, at: decl! };
      out.push(item);
    }
    return out;
  }

  /** Declares each snippet of the markup as a function member of the component. A call inside a snippet counts for the snippet. */
  private declareSnippets(
    ex: Extractor,
    sf: ts.SourceFile,
    c: Component,
    render: ts.FunctionDeclaration,
    comp: S.Symbol,
    prefix: string,
  ) {
    if (!c.snippets.length) return;
    const names = new Set(c.snippets.map(s => s.name));
    const visit = (n: ts.Node) => {
      if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name) && names.has(n.name.text) && n.initializer) {
        const fn = unwrap(n.initializer);
        const at = this.offsetOf(c, sf, n.getStart(sf));
        const sn = c.snippets.find(
          s => s.name === (n.name as ts.Identifier).text && at !== undefined && at >= s.start && at < s.end,
        );
        if (sn && ts.isArrowFunction(fn)) {
          const sig = ex.signature(sf, fn, "arrow", sn.name);
          sig.results = [];
          sig.text = `{#snippet ${sn.name}(${fn.parameters.map(p => collapse(p.getText(sf))).join(", ")})}`;
          const s = ex.addSym(
            {
              id: ex.symId(sf, prefix + sn.name),
              kind: "function",
              languageKind: "snippet",
              name: sn.name,
              parent: comp.id,
              exported: false,
              location: {
                path: ex.rel(sf.fileName),
                line: this.sourceLine(c, sn.start),
                endLine: this.sourceLine(c, sn.end - 1),
              },
              signature: sig,
            },
            prefix + sn.name,
          );
          ex.idOfNode.set(n, s.id);
          ex.idOfNode.set(fn, s.id);
          ex.callerOfNode.set(fn, s.id);
        }
      }
      ts.forEachChild(n, visit);
    };
    visit(render);
  }

  /** Declares each event that a component dispatches or forwards (the legacy `createEventDispatcher` and `on:` forms) as an event member, at the line that dispatches it. */
  private declareEvents(
    ex: Extractor,
    sf: ts.SourceFile,
    c: Component,
    render: ts.FunctionDeclaration,
    comp: S.Symbol,
    prefix: string,
  ) {
    if (!c.events.length) return;
    const dispatched = new Map<string, ts.Node>();
    const visit = (n: ts.Node) => {
      if (ts.isCallExpression(n) && n.arguments.length && ts.isStringLiteralLike(n.arguments[0])) {
        const ev = n.arguments[0].text;
        if (!dispatched.has(ev) && isDispatcher(ex, n.expression)) dispatched.set(ev, n);
      }
      ts.forEachChild(n, visit);
    };
    visit(render);
    for (const ev of [...c.events].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))) {
      const call = dispatched.get(ev.name);
      let line = call ? ex.line(call, sf) : 0;
      if (!line) {
        const m = new RegExp(`on:${ev.name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![\\w-])`).exec(c.source);
        line = m ? this.sourceLine(c, m.index) : 1;
      }
      const s = ex.addSym(
        {
          id: ex.symId(sf, prefix + ev.name),
          kind: "event",
          languageKind: "event",
          name: ev.name,
          parent: comp.id,
          exported: true,
          location: { path: ex.rel(sf.fileName), line },
          type: { text: ev.type },
        },
        prefix + ev.name,
      );
      if (ev.doc) ex.queue(ev.doc, sf, call ?? sf, d => (s.doc = d));
    }
  }

  /** The component that a node renders, when the node is svelte2tsx's call for a component tag; undefined otherwise. */
  renderTarget(ex: Extractor, n: ts.Node): { id: string; at: ts.Node } | undefined {
    if (!ts.isCallExpression(n) || !ts.isIdentifier(n.expression) || n.expression.text !== RENDER_CALL)
      return undefined;
    const arg = n.arguments[0] && unwrap(n.arguments[0]);
    if (!arg) return undefined;
    const at = ts.isPropertyAccessExpression(arg) ? arg.name : arg;
    const id = ex.idOfSymbol(ex.checker.getSymbolAtLocation(at));
    return id && ex.isCallable(id) ? { id, at } : undefined;
  }

  /** The limits that components add to a run, in plain words, for the output's notes. */
  limits(): string[] {
    const tool = (require("svelte2tsx/package.json") as { version: string }).version;
    const out = [
      `Components come from svelte2tsx ${tool} with svelte ${this.svelteVersion}, not from the project's own Svelte version.`,
      "A render of a component that a variable holds, or of a snippet that a prop passes in, does not appear.",
      ...this.notes,
    ];
    if (this.unresolvedTypes)
      out.push(
        `${this.unresolvedTypes} imports of the route types that svelte-kit sync writes (./$types) did not resolve, so the types they bring count as unknown.`,
      );
    return out;
  }

  /** The line of the first `{@render children()}` or `<slot>` in a component, where a layout renders the page inside it; 0 when it has none. */
  childLine(fileName: string): number {
    const c = this.get(fileName);
    const m = c && /\{@render\s+children\b|<slot[\s/>]/.exec(c.source);
    return c && m ? this.sourceLine(c, m.index) : 0;
  }
}

/** Reports whether the value of a `lang` attribute names TypeScript. */
function langIsTs(value: unknown): boolean {
  const parts = Array.isArray(value) ? value : [value];
  return parts.some(p => {
    const t = (p as { data?: unknown })?.data;
    return typeof t === "string" && /^(ts|typescript)$/i.test(t.trim());
  });
}

/** The offset at which each line of a text starts. */
function lineStarts(text: string): number[] {
  const out = [0];
  for (let i = 0; i < text.length; i++) if (text[i] === "\n") out.push(i + 1);
  return out;
}

/** Finds every snippet block in a component's markup, at any depth, in source order. */
function snippets(fragment: unknown): (Range & { name: string })[] {
  const out: (Range & { name: string })[] = [];
  const seen = new Set<unknown>();
  const visit = (v: unknown) => {
    if (!v || typeof v !== "object" || seen.has(v)) return;
    seen.add(v);
    if (Array.isArray(v)) {
      for (const x of v) visit(x);
      return;
    }
    const n = v as SvelteNode;
    if (n.type === "SnippetBlock" && n.expression?.name && n.start !== undefined && n.end !== undefined)
      out.push({ name: n.expression.name, start: n.start, end: n.end });
    for (const [k, x] of Object.entries(v)) if (k !== "parent" && k !== "metadata") visit(x);
  };
  visit(fragment);
  return out.sort((a, b) => a.start - b.start);
}

/** Reports whether a statement holds only names that svelte2tsx makes: the component's own constant and type, store reads, and names that start with "$" or "__sveltets". Svelte keeps names that start with "$" for itself. */
function isGenerated(st: ts.Statement, genName: string): boolean {
  const gen = (n: string) => n === genName || n.startsWith("$") || n.startsWith("__sveltets");
  if (ts.isVariableStatement(st))
    return st.declarationList.declarations.every(d => ts.isIdentifier(d.name) && gen(d.name.text));
  if (ts.isFunctionDeclaration(st) || ts.isTypeAliasDeclaration(st) || ts.isInterfaceDeclaration(st))
    return !!st.name && gen(st.name.text);
  return false;
}

/** Reports whether a statement only declares props: a legacy `export let`, which the component's parameters already show. */
function isPropVariable(st: ts.Statement, props: Set<string | undefined>): boolean {
  return (
    ts.isVariableStatement(st) &&
    st.declarationList.declarations.every(d => ts.isIdentifier(d.name) && props.has(d.name.text))
  );
}

/** The default value of each prop, as written: in the destructuring of `$props()`, or in a legacy `export let`. */
function propDefaults(render: ts.FunctionDeclaration): Map<string, string> {
  const out = new Map<string, string>();
  for (const st of render.body?.statements ?? []) {
    if (!ts.isVariableStatement(st)) continue;
    for (const d of st.declarationList.declarations) {
      if (ts.isObjectBindingPattern(d.name) && d.initializer && isPropsCall(d.initializer)) {
        for (const el of d.name.elements) {
          if (el.dotDotDotToken || !el.initializer) continue;
          const key = el.propertyName ?? el.name;
          if (ts.isIdentifier(key) || ts.isStringLiteral(key)) out.set(key.text, collapse(el.initializer.getText()));
        }
      } else if (ts.isIdentifier(d.name) && d.initializer && !out.has(d.name.text)) {
        out.set(d.name.text, collapse(d.initializer.getText()));
      }
    }
  }
  return out;
}

/** Reports whether an expression calls `$props()`. */
function isPropsCall(e: ts.Expression): boolean {
  const x = unwrap(e);
  return ts.isCallExpression(x) && ts.isIdentifier(x.expression) && x.expression.text === "$props";
}

/** Reports whether a callee holds what `createEventDispatcher()` returns. */
function isDispatcher(ex: Extractor, callee: ts.Expression): boolean {
  if (!ts.isIdentifier(callee)) return false;
  const d = ex.checker.getSymbolAtLocation(callee)?.valueDeclaration;
  if (!d || !ts.isVariableDeclaration(d) || !d.initializer) return false;
  const init = unwrap(d.initializer);
  return ts.isCallExpression(init) && /(^|\.)createEventDispatcher$/.test(init.expression.getText());
}

/** The text of the last JSDoc comment above a declaration, without its markers, or undefined. */
function lastDoc(decl: ts.Node, sf: ts.SourceFile): string | undefined {
  const docs = ts.getJSDocCommentsAndTags(decl).filter(ts.isJSDoc);
  const last = docs[docs.length - 1];
  return (last && commentText(sf.text.slice(last.pos, last.end))) || undefined;
}
