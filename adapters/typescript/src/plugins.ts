// Entry-point plugins find the ways into the code: HTTP routes. Each one
// switches on from the config file.
//
//   - express: app.get("/x", h), router.post(...), app.route("/x").get(h),
//     and app.use("/prefix", router) for a router that another one mounts.
//   - hono: app.get("/x", h), app.on("PURGE", "/x", h), app.route("/prefix",
//     sub) for a mounted app, and new Hono().basePath("/api").
//
// The last argument handles the request; the ones before it run as
// middleware. An inline handler inside a function names that function as the
// handler and counts as inline, as in the Go adapter. An inline handler at
// the top level of a module has no function around it, so it becomes a
// function symbol of its own, named after its route.

import ts from "typescript";

import { collapse, type Extractor, shorten, unwrap } from "./extract.js";

/** Switches on one entry-point plugin. */
export interface PluginConfig {
  plugin: string;
  options?: unknown;
}

interface PrefixRule {
  /** Routes registered inside the function or method of this name gain the prefix. */
  inFunc: string;
  prefix: string;
}

interface RouteOptions {
  prefixes: PrefixRule[];
}

type Framework = "express" | "hono";

const EXPRESS_VERBS: Record<string, string> = {
  get: "GET",
  post: "POST",
  put: "PUT",
  delete: "DELETE",
  patch: "PATCH",
  head: "HEAD",
  options: "OPTIONS",
  all: "ANY",
};
const HONO_VERBS: Record<string, string> = {
  get: "GET",
  post: "POST",
  put: "PUT",
  delete: "DELETE",
  patch: "PATCH",
  options: "OPTIONS",
  all: "ANY",
};
const CHAINABLE = new Set([...Object.keys(EXPRESS_VERBS), "use", "route", "on", "basePath"]);
const EXPRESS_TYPES = new Set(["Express", "Application", "Router", "IRouter", "Route", "IRoute"]);
const HONO_TYPES = new Set(["Hono", "HonoBase"]);

interface Registration {
  plugin: Framework;
  methods: string[];
  paths: string[];
  chainPrefix: string;
  key: ts.Symbol | undefined;
  handler: ts.Expression;
  caller: string | undefined;
  sf: ts.SourceFile;
  at: ts.Node;
}

interface Mount {
  parent: ts.Symbol | undefined;
  parentPrefix: string;
  child: ts.Symbol;
  prefix: string;
}

/** Joins two route paths with one slash between them. */
export function joinPath(a: string, b: string): string {
  if (!a) return b || "/";
  if (!b || b === "/") return a;
  return a.replace(/\/+$/, "") + (b.startsWith("/") ? b : "/" + b);
}

function routeOptions(plugin: string, raw: unknown): RouteOptions {
  const o = (raw ?? {}) as { prefixes?: unknown };
  const list = Array.isArray(o.prefixes) ? o.prefixes : [];
  const prefixes: PrefixRule[] = [];
  for (const r of list) {
    const rule = r as Partial<PrefixRule>;
    if (typeof rule?.inFunc !== "string" || typeof rule?.prefix !== "string")
      throw new Error(`the options of plugin ${plugin}: each prefix rule needs inFunc and prefix`);
    prefixes.push({ inFunc: rule.inFunc, prefix: rule.prefix });
  }
  return { prefixes };
}

/** The entry-point plugins of one run. */
export class Plugins {
  private readonly on: Partial<Record<Framework, RouteOptions>> = {};
  private readonly regs: Registration[] = [];
  private readonly mounts: Mount[] = [];

  constructor(configs: readonly PluginConfig[]) {
    for (const c of configs) {
      if (c.plugin !== "express" && c.plugin !== "hono")
        throw new Error(`the TypeScript adapter has no entry-point plugin named "${c.plugin}" (known: express, hono)`);
      this.on[c.plugin] = routeOptions(c.plugin, c.options);
    }
  }

  /** Finds the route registrations and mounts in one file. */
  scan(ex: Extractor, sf: ts.SourceFile) {
    if (!this.on.express && !this.on.hono) return;
    ex.walkWithCaller(sf, (n, caller) => {
      if (ts.isCallExpression(n) && ts.isPropertyAccessExpression(n.expression))
        this.call(ex, sf, n, n.expression, caller);
    });
  }

  private call(
    ex: Extractor,
    sf: ts.SourceFile,
    n: ts.CallExpression,
    callee: ts.PropertyAccessExpression,
    caller?: string,
  ) {
    const verb = callee.name.text;
    if (!CHAINABLE.has(verb) && verb !== "on") return;
    const { base, routePath, prefix } = chain(ex, callee.expression);
    const fw = this.framework(ex, base);
    if (!fw) return;
    const key = receiverSymbol(ex, base);
    const args = n.arguments;
    const reg = (methods: string[], paths: string[], handlers: readonly ts.Expression[]) => {
      if (!handlers.length || !paths.length || !methods.length) return;
      this.regs.push({
        plugin: fw,
        methods,
        paths,
        chainPrefix: prefix,
        key,
        handler: handlers[handlers.length - 1],
        caller,
        sf,
        at: callee.name,
      });
    };
    if (fw === "express") {
      if (verb === "use") {
        const p = args.length >= 2 ? strings(ex, args[0]) : undefined;
        const child = args.length >= 2 ? receiverSymbol(ex, unwrap(args[args.length - 1])) : undefined;
        if (p?.length === 1 && child) this.mounts.push({ parent: key, parentPrefix: prefix, child, prefix: p[0] });
        return;
      }
      const m = EXPRESS_VERBS[verb];
      if (!m) return;
      if (routePath !== undefined) reg([m], [routePath], args);
      else reg([m], strings(ex, args[0]) ?? [], args.slice(1));
      return;
    }
    if (verb === "route" && args.length === 2) {
      const p = strings(ex, args[0]);
      const child = receiverSymbol(ex, unwrap(args[1]));
      if (p?.length === 1 && child) this.mounts.push({ parent: key, parentPrefix: prefix, child, prefix: p[0] });
      return;
    }
    if (verb === "on") {
      reg(
        (strings(ex, args[0]) ?? []).map(x => x.toUpperCase()),
        strings(ex, args[1]) ?? [],
        args.slice(2),
      );
      return;
    }
    const m = HONO_VERBS[verb];
    if (m) reg([m], strings(ex, args[0]) ?? [], args.slice(1));
  }

  /** Tells which framework a receiver belongs to: by its type, or else by the import it comes from. */
  private framework(ex: Extractor, base: ts.Expression): Framework | undefined {
    const t = ex.checker.getTypeAtLocation(base);
    const syms = [t.aliasSymbol, t.getSymbol()];
    if (t.isClassOrInterface()) for (const b of ex.checker.getBaseTypes(t)) syms.push(b.getSymbol());
    for (const s of syms) {
      if (!s) continue;
      const file = s.declarations?.[0]?.getSourceFile().fileName ?? "";
      if (this.on.express && EXPRESS_TYPES.has(s.name) && /express/i.test(file)) return "express";
      if (this.on.hono && HONO_TYPES.has(s.name) && /hono/i.test(file)) return "hono";
    }
    const mod = originModule(ex, base, 0);
    if (this.on.express && mod === "express") return "express";
    if (this.on.hono && mod !== undefined && /^hono(\/|$)/.test(mod)) return "hono";
    return undefined;
  }

  /** The prefixes under which a receiver serves its routes: its mounts, then its own base path. */
  private prefixesOf(ex: Extractor, key: ts.Symbol | undefined, seen: Set<ts.Symbol>): string[] {
    if (!key || seen.has(key)) return [""];
    seen.add(key);
    const own = initPrefix(ex, key);
    const ms = this.mounts.filter(m => m.child === key);
    if (!ms.length) return [own];
    const out = ms.flatMap(m =>
      this.prefixesOf(ex, m.parent, new Set(seen)).map(p =>
        joinPath(joinPath(joinPath(p, m.parentPrefix), m.prefix), own),
      ),
    );
    return [...new Set(out)];
  }

  /** Turns the registrations into entry points, once every mount is known. */
  finish(ex: Extractor) {
    for (const r of this.regs) {
      const rule = this.on[r.plugin]!.prefixes.find(x => r.caller && ex.syms.get(r.caller)?.name === x.inFunc);
      const prefixes = this.prefixesOf(ex, r.key, new Set()).map(p =>
        joinPath(joinPath(rule?.prefix ?? "", p), r.chainPrefix),
      );
      const full: string[] = [];
      for (const pre of [...new Set(prefixes)]) for (const lp of r.paths) full.push(joinPath(pre, lp));
      const h = unwrap(r.handler);
      let handler: string | undefined;
      let inline = false;
      let external: string | undefined;
      const callable = ex.isCallable(r.caller);
      if (ts.isArrowFunction(h) || ts.isFunctionExpression(h)) {
        if (callable) {
          handler = r.caller;
          inline = true;
        } else handler = ex.inlineHandler(h, `${r.methods[0]} ${full[0]}`);
      } else {
        handler = firstCallable(ex, h);
        if (!handler) {
          external = shorten(collapse(h.getText(r.sf)));
          handler = callable ? r.caller : ex.idOfSymbol(r.key);
        }
      }
      if (!handler) continue;
      for (const p of full)
        for (const m of r.methods) {
          const name = `${m} ${p}`;
          const ep: (typeof ex.entries)[number] = {
            id: "http:" + name,
            kind: "http",
            name,
            method: m,
            path: p,
            handler,
            registeredAt: { path: ex.rel(r.sf.fileName), line: ex.line(r.at, r.sf) },
            plugin: r.plugin,
          };
          if (inline) ep.inline = true;
          if (external) ep.externalHandler = external;
          if (callable) ep.registrar = r.caller;
          ex.entries.push(ep);
        }
    }
  }
}

/** Walks a chain of calls (app.route("/x").get(h)) back to its receiver. */
function chain(ex: Extractor, e: ts.Expression): { base: ts.Expression; routePath?: string; prefix: string } {
  let routePath: string | undefined;
  let prefix = "";
  e = unwrap(e);
  while (ts.isCallExpression(e) && ts.isPropertyAccessExpression(e.expression)) {
    const nm = e.expression.name.text;
    if (nm === "route" && e.arguments.length === 1) routePath ??= strings(ex, e.arguments[0])?.[0];
    else if (nm === "basePath" && e.arguments.length === 1)
      prefix = joinPath(strings(ex, e.arguments[0])?.[0] ?? "", prefix);
    else if (!CHAINABLE.has(nm)) break;
    e = unwrap(e.expression.expression);
  }
  return { base: e, routePath, prefix };
}

/** The base path that a receiver's own initializer sets: const api = new Hono().basePath("/api"). */
function initPrefix(ex: Extractor, key: ts.Symbol): string {
  const d = key.valueDeclaration;
  const init = d && (ts.isVariableDeclaration(d) || ts.isPropertyDeclaration(d)) ? d.initializer : undefined;
  return init ? chain(ex, init).prefix : "";
}

/** The symbol that names a receiver (a variable or a property), through imports. */
function receiverSymbol(ex: Extractor, e: ts.Expression): ts.Symbol | undefined {
  const at = ts.isPropertyAccessExpression(e) ? e.name : ts.isIdentifier(e) ? e : undefined;
  let s = at && ex.checker.getSymbolAtLocation(at);
  if (s && s.flags & ts.SymbolFlags.Alias) s = ex.checker.getAliasedSymbol(s);
  return s;
}

/** The module a value comes from, by its import or by the initializer of the variable that holds it. */
function originModule(ex: Extractor, e: ts.Expression, depth: number): string | undefined {
  if (depth > 6) return undefined;
  e = unwrap(e);
  if (ts.isNewExpression(e) || ts.isCallExpression(e)) return originModule(ex, e.expression, depth + 1);
  if (ts.isPropertyAccessExpression(e)) {
    const s = ex.checker.getSymbolAtLocation(e.name);
    const d = s?.valueDeclaration;
    if (d && ts.isPropertyDeclaration(d) && d.initializer) return originModule(ex, d.initializer, depth + 1);
    return originModule(ex, e.expression, depth + 1);
  }
  if (!ts.isIdentifier(e)) return undefined;
  let s = ex.checker.getSymbolAtLocation(e);
  if (!s) return undefined;
  if (s.flags & ts.SymbolFlags.Alias) {
    const target = ex.checker.getAliasedSymbol(s);
    const td = target.valueDeclaration;
    if (td && ts.isVariableDeclaration(td) && td.initializer && !td.getSourceFile().isDeclarationFile)
      return originModule(ex, td.initializer, depth + 1);
    let n: ts.Node | undefined = s.declarations?.[0];
    while (n && !ts.isImportDeclaration(n) && !ts.isImportEqualsDeclaration(n)) n = n.parent;
    if (n && ts.isImportDeclaration(n) && ts.isStringLiteralLike(n.moduleSpecifier)) return n.moduleSpecifier.text;
    if (n && ts.isImportEqualsDeclaration(n) && ts.isExternalModuleReference(n.moduleReference)) {
      const x = n.moduleReference.expression;
      return ts.isStringLiteralLike(x) ? x.text : undefined;
    }
    return undefined;
  }
  s = ex.checker.getExportSymbolOfSymbol(s);
  const d = s.valueDeclaration;
  if (d && ts.isVariableDeclaration(d) && d.initializer) return originModule(ex, d.initializer, depth + 1);
  return undefined;
}

/** The string values of an argument: a literal, a constant of a literal type, or an array of them. */
function strings(ex: Extractor, e: ts.Expression | undefined): string[] | undefined {
  if (!e) return undefined;
  e = unwrap(e);
  if (ts.isStringLiteralLike(e)) return [e.text];
  if (ts.isArrayLiteralExpression(e)) {
    const out: string[] = [];
    for (const x of e.elements) {
      const v = strings(ex, x);
      if (!v) return undefined;
      out.push(...v);
    }
    return out;
  }
  const t = ex.checker.getTypeAtLocation(e);
  if (t.isStringLiteral()) return [t.value];
  return undefined;
}

/** The ID of the first project function or method that a handler expression names: list in asyncHandler(controller.list). */
function firstCallable(ex: Extractor, e: ts.Expression): string | undefined {
  let found: string | undefined;
  const visit = (n: ts.Node) => {
    if (found || ts.isArrowFunction(n) || ts.isFunctionExpression(n)) return;
    if (ts.isIdentifier(n)) {
      const id = ex.idOfSymbol(ex.checker.getSymbolAtLocation(n));
      if (ex.isCallable(id)) {
        found = id;
        return;
      }
    }
    ts.forEachChild(n, visit);
  };
  if (ts.isCallExpression(e)) {
    for (const a of e.arguments) visit(a);
    if (!found) visit(e.expression);
  } else visit(e);
  return found;
}
