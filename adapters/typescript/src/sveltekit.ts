// The sveltekit entry-point plugin: reads SvelteKit's file-based routes from
// file names under src/routes (option "routes"). It never runs
// svelte.config.js or vite.config.js.
//
//   - +server.ts or +server.js: each exported GET, POST, PUT, PATCH, DELETE,
//     OPTIONS and HEAD makes an http entry with that method; fallback makes
//     ANY.
//   - A page (+page.svelte, with optional +page.ts and +page.server.ts)
//     makes GET <path>. Its handler: the load of +page.server.ts, else the
//     load of +page.ts, else the component.
//   - Form actions in +page.server.ts: default makes POST <path>; a named
//     action makes POST <path>?/<name>.
//
// Layouts make no entry: a layout answers no request alone. SvelteKit
// renders each page inside the layouts above it, and a load that calls
// parent() reads the data of the layouts' loads. No checker resolves either
// use, so the plugin records both as calls edges with heuristic confidence.

import path from "node:path";
import ts from "typescript";

import { type Extractor, unwrap } from "./extract.js";
import { firstCallable } from "./plugins.js";

/** The HTTP methods that a +server file may export, each by its own name. */
const VERBS = new Set(["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "HEAD"]);
/** Matches a page or layout component: the kind, then the segment after "@" that resets its layouts, if any. */
const COMPONENT = /^\+(page|layout)(?:@(.*))?\.svelte$/;
/** Matches a route module: +server, or a page or layout module, on the server or not. */
const MODULE = /^\+(?:(server)|(page|layout)(\.server)?)\.(?:js|ts)$/;

/** One page or one layout: its component and its two modules, any of which may be absent. */
interface RouteNode {
  /** The component, +page.svelte or +layout.svelte. */
  component?: ts.SourceFile;
  /** The segment after "@" in the component's name: the layout that the node resets to; "" names the root. */
  uses?: string;
  /** The universal module, +page.ts or +layout.ts. */
  universal?: ts.SourceFile;
  /** The server module, +page.server.ts or +layout.server.ts. */
  server?: ts.SourceFile;
}

/** One folder under the routes folder. */
interface Folder {
  /** The folder's path from the routes folder; "" for the routes folder itself. */
  dir: string;
  /** The folder's own name; "" for the routes folder. */
  segment: string;
  /** The folder above, if any. */
  parent?: Folder;
  /** The folder's layout. */
  layout?: RouteNode;
  /** The folder's page. */
  page?: RouteNode;
  /** The folder's +server module. */
  endpoint?: ts.SourceFile;
}

/** Reads a SvelteKit app's routes, and records its entry points and layout edges. */
export class SvelteKit {
  /** The routes folder, relative to the root, without a trailing slash. */
  private readonly routes: string;

  /** Reads the plugin's options, and fails with a message when `routes` holds no string. */
  constructor(options: unknown) {
    const o = (options ?? {}) as { routes?: unknown };
    if (o.routes !== undefined && typeof o.routes !== "string")
      throw new Error(`the options of plugin sveltekit: routes needs a folder path, such as "src/routes"`);
    this.routes = path.posix.normalize(o.routes ?? "src/routes").replace(/^\.\/|\/+$/g, "");
  }

  /** Finds the routes among the map's files, and adds their entry points and edges. */
  finish(ex: Extractor) {
    const folders = this.folders(ex);
    const added = new Set<string>();
    const edge = (from: string | undefined, to: string | undefined, line: number) => {
      if (!from || !to || added.has(from + "\0" + to)) return;
      added.add(from + "\0" + to);
      ex.addEdge(from, to, "calls", line, "heuristic");
    };
    for (const f of folders) {
      const url = urlPath(f.dir);
      if (f.endpoint) this.endpoint(ex, f.endpoint, url);
      if (f.page) this.page(ex, f.page, url);
      // Each layout renders the nearest layout or page below it.
      if (f.page?.component) {
        const chain = [...layouts(f, f.page.uses).filter(n => n.component), f.page];
        for (let i = 0; i + 1 < chain.length; i++) {
          const outer = chain[i].component!;
          edge(componentId(ex, outer), componentId(ex, chain[i + 1].component!), ex.svelte!.childLine(outer.fileName));
        }
      }
      // A load that calls parent() reads the loads of the layouts around it.
      for (const [node, around] of [
        [f.page, f.page && layouts(f, f.page.uses)],
        [f.layout, f.layout && layouts(f.parent, f.layout.uses)],
      ] as const) {
        if (!node || !around) continue;
        const server = node.server && loadOf(ex, node.server);
        if (server?.parentCall)
          for (const l of around) edge(server.id, l.server && loadOf(ex, l.server)?.id, ex.line(server.parentCall));
        const universal = node.universal && loadOf(ex, node.universal);
        if (universal?.parentCall)
          for (const l of around) {
            const target = (l.universal && loadOf(ex, l.universal)) || (l.server && loadOf(ex, l.server));
            edge(universal.id, target?.id, ex.line(universal.parentCall));
          }
      }
    }
  }

  /** Builds the folder tree from the route files among the map's files. */
  private folders(ex: Extractor): Folder[] {
    const byDir = new Map<string, Folder>();
    const folder = (dir: string): Folder => {
      let f = byDir.get(dir);
      if (f) return f;
      f = { dir, segment: dir ? path.posix.basename(dir) : "" };
      if (dir) f.parent = folder(path.posix.dirname(dir) === "." ? "" : path.posix.dirname(dir));
      byDir.set(dir, f);
      return f;
    };
    for (const sf of ex.files) {
      const r = ex.relRoot(sf.fileName);
      if (!r.startsWith(this.routes + "/")) continue;
      const rest = r.slice(this.routes.length + 1);
      const dir = path.posix.dirname(rest) === "." ? "" : path.posix.dirname(rest);
      const base = path.posix.basename(rest);
      const c = COMPONENT.exec(base);
      if (c && ex.svelte?.get(sf.fileName)?.id) {
        const f = folder(dir);
        const node = c[1] === "page" ? (f.page ??= {}) : (f.layout ??= {});
        node.component = sf;
        if (c[2] !== undefined) node.uses = c[2];
        continue;
      }
      const m = MODULE.exec(base);
      if (!m) continue;
      const f = folder(dir);
      if (m[1]) f.endpoint = sf;
      else {
        const node = m[2] === "page" ? (f.page ??= {}) : (f.layout ??= {});
        if (m[3]) node.server = sf;
        else node.universal = sf;
      }
    }
    return [...byDir.values()].sort((a, b) => (a.dir < b.dir ? -1 : a.dir > b.dir ? 1 : 0));
  }

  /** Adds an entry for each HTTP method that a +server module exports; fallback answers every other method. */
  private endpoint(ex: Extractor, sf: ts.SourceFile, url: string) {
    for (const e of exportsOf(ex, sf)) {
      const method = e.name === "fallback" ? "ANY" : VERBS.has(e.name) ? e.name : undefined;
      const handler = method && handlerOf(ex, e);
      if (!method || !handler) continue;
      add(ex, method, url, handler, sf, e.declarations?.[0] ?? sf);
    }
  }

  /** Adds the GET entry of a page, and an entry for each form action of its server module. */
  private page(ex: Extractor, node: RouteNode, url: string) {
    const handler =
      (node.server && loadOf(ex, node.server)?.id) ||
      (node.universal && loadOf(ex, node.universal)?.id) ||
      (node.component && componentId(ex, node.component));
    const at = node.component ?? node.server ?? node.universal;
    if (handler && at) add(ex, "GET", url, handler, at, at);
    if (!node.server) return;
    const actions = exportsOf(ex, node.server).find(e => e.name === "actions");
    const decl = actions?.valueDeclaration;
    const init = decl && ts.isVariableDeclaration(decl) && decl.initializer && unwrap(decl.initializer);
    if (!init || !ts.isObjectLiteralExpression(init)) return;
    for (const p of init.properties) {
      const name = p.name && (ts.isIdentifier(p.name) || ts.isStringLiteralLike(p.name)) ? p.name.text : undefined;
      if (!name) continue;
      const target = name === "default" ? url : `${url}?/${name}`;
      let h: string | undefined;
      if (ts.isMethodDeclaration(p)) h = ex.inlineHandler(p, `POST ${target}`);
      else if (ts.isShorthandPropertyAssignment(p)) h = ex.idOfSymbol(ex.checker.getShorthandAssignmentValueSymbol(p));
      else if (ts.isPropertyAssignment(p)) {
        const v = unwrap(p.initializer);
        if (ts.isArrowFunction(v) || ts.isFunctionExpression(v)) h = ex.inlineHandler(v, `POST ${target}`);
        else if (ts.isCallExpression(v)) h = firstCallable(ex, v);
        else h = ex.idOfSymbol(ex.checker.getSymbolAtLocation(ts.isPropertyAccessExpression(v) ? v.name : v));
      }
      if (ex.isCallable(h)) add(ex, "POST", target, h!, node.server, p.name!);
    }
  }
}

/** Adds one http entry point of the sveltekit plugin. */
function add(ex: Extractor, method: string, url: string, handler: string, sf: ts.SourceFile, at: ts.Node) {
  const name = `${method} ${url}`;
  ex.entries.push({
    id: "http:" + name,
    kind: "http",
    name,
    method,
    path: url,
    handler,
    registeredAt: { path: ex.rel(sf.fileName), line: ts.isSourceFile(at) ? 1 : ex.line(at, sf) },
    plugin: "sveltekit",
  });
}

/** The URL path of a folder under the routes folder: its names joined, without the (group) folders, which leave the URL alone. */
export function urlPath(dir: string): string {
  return (
    "/" +
    dir
      .split("/")
      .filter(s => s && !/^\(.*\)$/.test(s))
      .join("/")
  );
}

/**
 * The layouts around a page or a layout, outermost first, as SvelteKit
 * chooses them: from `start` up to the routes folder, each folder's layout,
 * except that a name after "@" skips the folders up to the one of that name.
 */
function layouts(start: Folder | undefined, uses: string | undefined): RouteNode[] {
  const out: RouteNode[] = [];
  let want = uses;
  for (let f = start; f; f = f.parent) {
    if (want !== undefined && f.segment !== want) continue;
    if (f.layout) {
      out.unshift(f.layout);
      want = f.layout.uses;
    } else want = undefined;
  }
  return out;
}

/** The ID of the component that a page or layout file declares. */
function componentId(ex: Extractor, sf: ts.SourceFile): string | undefined {
  return ex.svelte?.get(sf.fileName)?.id;
}

/** The exports of a module, sorted by name. */
function exportsOf(ex: Extractor, sf: ts.SourceFile): ts.Symbol[] {
  const ms = ex.checker.getSymbolAtLocation(sf);
  const list = ms ? ex.checker.getExportsOfModule(ms) : [];
  return [...list].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
}

/**
 * The function that serves an export: the export itself when it holds a
 * function, or else the first project function that its initializer names,
 * as rssHandler in `export const GET = rssHandler(…)`.
 */
function handlerOf(ex: Extractor, sym: ts.Symbol): string | undefined {
  const id = ex.idOfSymbol(sym);
  if (ex.isCallable(id)) return id;
  const target = sym.flags & ts.SymbolFlags.Alias ? ex.checker.getAliasedSymbol(sym) : sym;
  const d = target.valueDeclaration;
  return d && ts.isVariableDeclaration(d) && d.initializer ? firstCallable(ex, unwrap(d.initializer)) : undefined;
}

/** The `load` function that a route module exports, and its first call of parent(), if any. */
function loadOf(ex: Extractor, sf: ts.SourceFile): { id: string; parentCall?: ts.Node } | undefined {
  let sym = exportsOf(ex, sf).find(e => e.name === "load");
  const id = sym && handlerOf(ex, sym);
  if (!sym || !id) return undefined;
  if (sym.flags & ts.SymbolFlags.Alias) sym = ex.checker.getAliasedSymbol(sym);
  const d = sym.valueDeclaration;
  const body = d && ts.isVariableDeclaration(d) ? d.initializer : d;
  let parentCall: ts.Node | undefined;
  const visit = (n: ts.Node) => {
    if (parentCall) return;
    if (ts.isCallExpression(n)) {
      const e = n.expression;
      if (
        (ts.isIdentifier(e) && e.text === "parent") ||
        (ts.isPropertyAccessExpression(e) && e.name.text === "parent")
      ) {
        parentCall = n;
        return;
      }
    }
    ts.forEachChild(n, visit);
  };
  if (body) visit(body);
  return { id, parentCall };
}
