// The JavaScript rules of the adapter. The compiler binds JavaScript by
// itself: CommonJS exports, require calls, prototype assignments, this.x
// properties and the JSDoc type tags. The declaration pass in extract.ts only
// reads declarations, so this file adds the symbols that JavaScript writes as
// assignments and in JSDoc:
//
//   - module.exports = …, exports.f = …, and module.exports = { … };
//   - constructor functions, with their prototype, static and this.x members;
//   - object literals that act as modules, and their expando members;
//   - this.x = … in the constructor of an ES class;
//   - @typedef and @callback, with their @property and @param tags.
//
// docs/plans/phase-5-javascript.md records the decisions.

import path from "node:path";
import ts from "typescript";

import { collapse, type Extractor, shorten, unwrap } from "./extract.js";
import type * as S from "./schema.js";

/** Reports whether a node is a function written as a value: an arrow function or a function expression. */
function isFunctionValue(e: ts.Node | undefined): e is ts.ArrowFunction | ts.FunctionExpression {
  return !!e && (ts.isArrowFunction(e) || ts.isFunctionExpression(e));
}

/** Reports whether a call is require("x") with one argument. */
export function isRequire(n: ts.Node): n is ts.CallExpression {
  return (
    ts.isCallExpression(n) &&
    ts.isIdentifier(n.expression) &&
    n.expression.text === "require" &&
    n.arguments.length === 1
  );
}

/** The module that an expression requires: "express" in require("express") or require("express").Router. */
export function requireSpec(e: ts.Expression | undefined): string | undefined {
  if (!e) return undefined;
  e = unwrap(e);
  while (ts.isPropertyAccessExpression(e)) e = unwrap(e.expression);
  if (isRequire(e) && ts.isStringLiteralLike(e.arguments[0])) return e.arguments[0].text;
  return undefined;
}

/** The type node that JSDoc writes for a declaration: @param {T} for a parameter, @returns {T} for a getter, @type {T} for the rest. An optional (T=) or rest (...T) mark drops out. */
export function jsDocTypeNode(decl: ts.Node): ts.TypeNode | undefined {
  let t: ts.TypeNode | undefined;
  if (ts.isJSDocPropertyLikeTag(decl)) t = decl.typeExpression?.type;
  else if (ts.isGetAccessor(decl)) t = ts.getJSDocReturnType(decl) ?? ts.getJSDocType(decl);
  else if (
    (ts.isPropertyAccessExpression(decl) || ts.isElementAccessExpression(decl)) &&
    ts.isBinaryExpression(decl.parent) &&
    decl.parent.left === decl
  )
    t = ts.getJSDocType(decl.parent) ?? ts.getJSDocType(decl.parent.parent);
  else t = ts.getJSDocType(decl);
  if (t && (ts.isJSDocOptionalType(t) || ts.isJSDocVariadicType(t))) t = t.type;
  return t;
}

/** Reports whether JSDoc hides a member: @private, @protected, or @access private. */
function jsDocHidden(node: ts.Node): boolean {
  return ts
    .getJSDocTags(node)
    .some(
      t =>
        t.tagName.text === "private" ||
        t.tagName.text === "protected" ||
        (t.tagName.text === "access" && /^\s*(private|protected)\b/.test(ts.getTextOfJSDocComment(t.comment) ?? "")),
    );
}

/** Reports whether a member counts as exported in JavaScript: not hidden by JSDoc, and not named with # or with _, which marks a private member by convention. */
function visible(name: string, ...nodes: ts.Node[]): boolean {
  return !name.startsWith("#") && !name.startsWith("_") && !nodes.some(jsDocHidden);
}

/** The name of a property as written: an identifier, a string or a number; undefined for a computed name. */
function propName(n: ts.PropertyName | ts.MemberName | ts.Expression | undefined): string | undefined {
  if (!n) return undefined;
  if (ts.isIdentifier(n) || ts.isPrivateIdentifier(n) || ts.isStringLiteralLike(n) || ts.isNumericLiteral(n))
    return n.text;
  return undefined;
}

/** The name that the left side of an assignment sets: x in a.x or a["x"]; undefined for a computed name. */
function assignedName(left: ts.Expression): string | undefined {
  if (ts.isPropertyAccessExpression(left)) return left.name.text;
  if (ts.isElementAccessExpression(left)) return propName(left.argumentExpression);
  return undefined;
}

/** Reports whether an expression names the exports object: exports or module.exports. */
function isExportsObject(e: ts.Expression): boolean {
  e = unwrap(e);
  return (ts.isIdentifier(e) && e.text === "exports") || isModuleExports(e);
}

/** Reports whether an expression reads module.exports. */
function isModuleExports(e: ts.Expression): boolean {
  e = unwrap(e);
  return (
    ts.isPropertyAccessExpression(e) &&
    e.name.text === "exports" &&
    ts.isIdentifier(e.expression) &&
    e.expression.text === "module"
  );
}

/** Reports whether an expression is X.prototype, and returns X. */
function prototypeOf(e: ts.Expression): ts.Expression | undefined {
  e = unwrap(e);
  return ts.isPropertyAccessExpression(e) && e.name.text === "prototype" ? e.expression : undefined;
}

/** Skips the targets of a chain of assignments: {} in exports = module.exports = {}. */
function lastValue(e: ts.Expression): ts.Expression {
  e = unwrap(e);
  while (ts.isBinaryExpression(e) && e.operatorToken.kind === ts.SyntaxKind.EqualsToken) e = unwrap(e.right);
  return e;
}

/** The assignments of `this.x = …` in a function body. The walk enters arrow functions, whose this stays the same, and skips other functions and classes. */
function thisAssignments(body: ts.Node): ts.BinaryExpression[] {
  const out: ts.BinaryExpression[] = [];
  const visit = (n: ts.Node) => {
    if (ts.isFunctionDeclaration(n) || ts.isFunctionExpression(n) || ts.isClassLike(n) || ts.isMethodDeclaration(n))
      return;
    if (
      ts.isBinaryExpression(n) &&
      n.operatorToken.kind === ts.SyntaxKind.EqualsToken &&
      (ts.isPropertyAccessExpression(n.left) || ts.isElementAccessExpression(n.left)) &&
      n.left.expression.kind === ts.SyntaxKind.ThisKeyword
    )
      out.push(n);
    ts.forEachChild(n, visit);
  };
  ts.forEachChild(body, visit);
  return out;
}

/** The node that carries the doc and the location of an assignment: its statement, when it stands alone. */
function statementOf(n: ts.Node): ts.Node {
  return ts.isExpressionStatement(n.parent) ? n.parent : n;
}

/** Declares the JavaScript symbols of one file that the declaration pass does not see. Runs after that pass. */
export function declareJavaScript(ex: Extractor, sf: ts.SourceFile, fileId: string) {
  new JsFile(ex, sf, fileId).run();
}

/**
 * Declares an object literal that holds a function as a namespace of kind
 * "object", with its properties as members: `const helpers = { find() {} }`.
 * Returns undefined, and declares nothing, when the literal holds no function.
 */
export function objectNamespace(
  ex: Extractor,
  sf: ts.SourceFile,
  decl: ts.VariableDeclaration,
  obj: ts.ObjectLiteralExpression,
  parent: string,
  p: string,
  exported: boolean,
  st: ts.Node,
): S.Symbol | undefined {
  if (!holdsFunction(obj)) return undefined;
  const { s } = ex.topSymbol(sf, decl, "variable", p, () => ({
    id: ex.symId(sf, p),
    kind: "namespace",
    languageKind: "object",
    name: propName(decl.name as ts.Identifier) ?? p,
    parent,
    exported,
    location: ex.loc(sf, st),
  }));
  ex.idOfNode.set(obj, s.id);
  new JsFile(ex, sf, parent).objectMembers(s, p, obj, false);
  return s;
}

/** Reports whether an object literal holds a method or a property whose value is a function. */
function holdsFunction(obj: ts.ObjectLiteralExpression): boolean {
  return obj.properties.some(
    m =>
      ts.isMethodDeclaration(m) ||
      ts.isAccessor(m) ||
      (ts.isPropertyAssignment(m) && isFunctionValue(unwrap(m.initializer))),
  );
}

/** The JavaScript rules for one file. */
class JsFile {
  /** Prepares the rules for a file whose container has the ID fileId. */
  constructor(
    private readonly ex: Extractor,
    private readonly sf: ts.SourceFile,
    private readonly fileId: string,
  ) {}

  /** Declares the typedefs, the constructor functions, the assignments at the top level, and the this.x members of classes. */
  run() {
    this.typedefs();
    for (const st of this.sf.statements) {
      const fn = this.functionOf(st);
      if (fn && this.isConstructor(fn)) this.promote(fn);
    }
    for (const st of this.sf.statements) {
      if (!ts.isExpressionStatement(st)) continue;
      const e = st.expression;
      if (ts.isBinaryExpression(e) && e.operatorToken.kind === ts.SyntaxKind.EqualsToken) this.assignment(e);
    }
    for (const st of this.sf.statements) {
      if (!ts.isClassDeclaration(st)) continue;
      const id = this.ex.idOfNode.get(st);
      const ctor = st.members.find((m): m is ts.ConstructorDeclaration => ts.isConstructorDeclaration(m) && !!m.body);
      if (id && ctor) this.thisMembers(id, ctor);
    }
  }

  /** The function that a top-level statement declares: function F() {}, or const F = function () {}. */
  private functionOf(st: ts.Statement): ts.FunctionDeclaration | ts.FunctionExpression | undefined {
    if (ts.isFunctionDeclaration(st) && st.body) return st;
    if (ts.isVariableStatement(st) && st.declarationList.declarations.length === 1) {
      const init = st.declarationList.declarations[0].initializer;
      const f = init && unwrap(init);
      if (f && ts.isFunctionExpression(f)) return f;
    }
    return undefined;
  }

  /** Reports whether a function acts as a class: JSDoc marks it @constructor or @class, its body sets this.x, or the file assigns to its prototype. */
  private isConstructor(fn: ts.FunctionDeclaration | ts.FunctionExpression): boolean {
    const doc = ts.isFunctionExpression(fn) ? fn.parent.parent : fn;
    if (ts.getJSDocTags(doc).some(t => t.tagName.text === "constructor" || t.tagName.text === "class")) return true;
    if (fn.body && thisAssignments(fn.body).length) return true;
    const sym = this.ex.checker.getSymbolAtLocation(fn.name ?? (fn.parent as ts.VariableDeclaration).name);
    return this.sf.statements.some(st => {
      if (!ts.isExpressionStatement(st) || !ts.isBinaryExpression(st.expression)) return false;
      const left = unwrap(st.expression.left);
      const x = prototypeOf(left) ?? (ts.isPropertyAccessExpression(left) ? prototypeOf(left.expression) : undefined);
      return !!x && ts.isIdentifier(x) && this.ex.checker.getSymbolAtLocation(x) === sym;
    });
  }

  /** Turns the function symbol of a constructor function into a class, with a constructor member that new reaches. */
  private promote(fn: ts.FunctionDeclaration | ts.FunctionExpression) {
    const ex = this.ex;
    const id = ex.idOfNode.get(fn);
    const s = id ? ex.syms.get(id) : undefined;
    if (!s || s.kind !== "function") return;
    const p = ex.pathOfId(s.id)!;
    s.kind = "class";
    s.languageKind = "constructor function";
    delete s.signature;
    const ctor = ex.addSym(
      {
        id: ex.symId(this.sf, `${p}.constructor`),
        kind: "constructor",
        languageKind: "constructor",
        name: "constructor",
        parent: s.id,
        exported: true,
        location: ex.loc(this.sf, fn),
        signature: ex.signature(this.sf, fn, "constructor", "constructor"),
      },
      `${p}.constructor`,
    );
    ex.callerOfNode.set(fn, ctor.id);
    ex.ctorOf.set(fn, ctor.id);
    const doc = ts.isFunctionExpression(fn) ? fn.parent : fn;
    ex.afterDeclarations(() => ex.jsDocHeritage(s.id, doc));
    if (fn.body) this.thisMembers(s.id, fn);
  }

  /** Declares a member for each `this.x = …` in a constructor's body, unless the class already declares one of that name. */
  private thisMembers(ownerId: string, fn: ts.FunctionLikeDeclaration) {
    const ex = this.ex;
    const p = ex.pathOfId(ownerId)!;
    const taken = new Set([...ex.syms.values()].filter(s => s.parent === ownerId).map(s => s.name));
    for (const a of thisAssignments(fn.body!)) {
      const name = assignedName(a.left);
      if (name === undefined || taken.has(name)) continue;
      taken.add(name);
      const value = unwrap(a.right);
      // A this.x of a constructor sits in its body, so its calls count for the constructor.
      this.member(ownerId, p, name, a, value, { isStatic: false, method: false, valueCalls: false });
    }
  }

  /** Reads one assignment at the top level: an export, a prototype member, or a static member. */
  private assignment(a: ts.BinaryExpression) {
    const left = unwrap(a.left);
    if (isModuleExports(left) || (ts.isIdentifier(left) && left.text === "exports")) {
      this.moduleExports(a, lastValue(a.right));
      return;
    }
    if (!ts.isPropertyAccessExpression(left) && !ts.isElementAccessExpression(left)) return;
    const target = unwrap(left.expression);
    const name = assignedName(left);
    if (isExportsObject(target)) {
      if (name !== undefined) this.namedExport(name, statementOf(a), left, a.right);
      return;
    }
    const proto = prototypeOf(target);
    if (proto) {
      const owner = this.owner(proto, true);
      if (owner && name !== undefined)
        this.member(owner.id, this.ex.pathOfId(owner.id)!, name, a, unwrap(a.right), {
          isStatic: false,
          method: true,
          valueCalls: true,
        });
      return;
    }
    if (name === "prototype") {
      const owner = this.owner(target, true);
      const obj = lastValue(a.right);
      if (owner && ts.isObjectLiteralExpression(obj)) this.objectMembers(owner, this.ex.pathOfId(owner.id)!, obj, true);
      return;
    }
    const owner = this.owner(target, false);
    if (owner && name !== undefined)
      this.member(owner.id, this.ex.pathOfId(owner.id)!, name, a, unwrap(a.right), {
        isStatic: owner.kind !== "namespace",
        method: false,
        valueCalls: true,
      });
  }

  /**
   * Finds the symbol of this file's top level that an assignment adds a
   * member to. A variable that holds an object literal becomes a namespace
   * of kind "object"; with `proto`, a function becomes a constructor
   * function.
   */
  private owner(e: ts.Expression, proto: boolean): S.Symbol | undefined {
    const ex = this.ex;
    if (!ts.isIdentifier(e)) return undefined;
    const sym = ex.checker.getSymbolAtLocation(e);
    const id = ex.idOfSymbol(sym);
    const s = id ? ex.syms.get(id) : undefined;
    if (!s || s.parent !== this.fileId || s.location.path !== ex.rel(this.sf.fileName)) return undefined;
    if (s.kind === "function" && proto) {
      const d = sym?.valueDeclaration;
      const fn = d && ts.isVariableDeclaration(d) && d.initializer ? unwrap(d.initializer) : d;
      if (fn && (ts.isFunctionDeclaration(fn) || ts.isFunctionExpression(fn))) this.promote(fn);
    }
    if (s.kind === "variable" || s.kind === "constant") {
      const d = sym?.valueDeclaration;
      const init = d && ts.isVariableDeclaration(d) && d.initializer ? lastValue(d.initializer) : undefined;
      if (!init || !ts.isObjectLiteralExpression(init)) return undefined;
      s.kind = "namespace";
      s.languageKind = "object";
      delete s.type;
      delete s.value;
      delete s.group;
      this.objectMembers(s, ex.pathOfId(s.id)!, init, false);
    }
    return ["function", "class", "namespace"].includes(s.kind) ? s : undefined;
  }

  /**
   * Declares one member that an assignment writes: `F.prototype.m = …`,
   * `F.m = …` or `this.x = …`. A function value makes a method; another
   * value makes a property. With `valueCalls`, the calls in the value count
   * for the member.
   */
  private member(
    ownerId: string,
    ownerPath: string,
    name: string,
    a: ts.BinaryExpression,
    value: ts.Expression,
    how: { isStatic: boolean; method: boolean; valueCalls: boolean },
  ) {
    const ex = this.ex;
    const sf = this.sf;
    const at = statementOf(a);
    const p = `${ownerPath}.${name}`;
    const s: S.Symbol = {
      id: ex.symId(sf, p),
      kind: "property",
      languageKind: "property",
      name,
      parent: ownerId,
      exported: visible(name, at, a),
      location: ex.loc(sf, at),
    };
    const mods = how.isStatic ? ["static"] : [];
    if (isFunctionValue(value)) {
      s.kind = "method";
      s.languageKind = how.method ? "method" : ts.isArrowFunction(value) ? "arrow function" : "function";
      mods.push(...ex.modifiers(value));
      s.signature = ex.signature(sf, value, ts.isArrowFunction(value) ? "property" : "method", name);
      if (how.isStatic && !ts.isArrowFunction(value)) s.signature.text = "static " + s.signature.text;
    } else s.type = ex.typeOf(sf, a.left, undefined);
    if (mods.length) s.modifiers = mods;
    ex.addSym(s, p);
    ex.idOfNode.set(a.left, s.id);
    ex.idOfNode.set(a, s.id);
    if (isFunctionValue(value)) {
      ex.idOfNode.set(value, s.id);
      ex.callerOfNode.set(value, s.id);
    } else if (how.valueCalls) ex.callerOfNode.set(value, s.id);
    ex.queueDecl(at, sf, s);
  }

  /** Declares the properties of an object literal as members of a symbol: an object namespace, or a prototype with `proto`. */
  objectMembers(owner: S.Symbol, ownerPath: string, obj: ts.ObjectLiteralExpression, proto: boolean) {
    const ex = this.ex;
    const sf = this.sf;
    const seen = new Map<string, S.Symbol>();
    for (const m of obj.properties) {
      if (ts.isSpreadAssignment(m)) continue;
      const name = propName(m.name);
      if (name === undefined) continue;
      const prior = seen.get(name);
      if (prior && ts.isAccessor(m)) {
        // The other half of a get and set pair.
        const word = ts.isGetAccessor(m) ? "get" : "set";
        if (!prior.modifiers?.includes(word)) (prior.modifiers ??= []).push(word);
        ex.idOfNode.set(m, prior.id);
        if (m.body) ex.callerOfNode.set(m, prior.id);
        continue;
      }
      const p = `${ownerPath}.${name}`;
      const s: S.Symbol = {
        id: ex.symId(sf, p),
        kind: "property",
        languageKind: "property",
        name,
        parent: owner.id,
        exported: visible(name, m),
        location: ex.loc(sf, m),
      };
      const init = ts.isPropertyAssignment(m) ? unwrap(m.initializer) : undefined;
      if (ts.isMethodDeclaration(m)) {
        s.kind = "method";
        s.languageKind = "method";
        s.signature = ex.signature(sf, m, "method", name);
        const mods = ex.modifiers(m);
        if (mods.length) s.modifiers = mods;
        if (m.body) ex.callerOfNode.set(m, s.id);
      } else if (ts.isAccessor(m)) {
        s.languageKind = "accessor";
        s.modifiers = [ts.isGetAccessor(m) ? "get" : "set"];
        s.type = ex.typeOf(sf, m, undefined);
        if (m.body) ex.callerOfNode.set(m, s.id);
      } else if (isFunctionValue(init)) {
        s.kind = "method";
        s.languageKind = proto ? "method" : ts.isArrowFunction(init) ? "arrow function" : "function";
        s.signature = ex.signature(sf, init, ts.isArrowFunction(init) ? "property" : "method", name);
        const mods = ex.modifiers(init);
        if (mods.length) s.modifiers = mods;
        ex.idOfNode.set(init, s.id);
        ex.callerOfNode.set(init, s.id);
      } else {
        s.type = ex.typeOf(sf, m, undefined);
        if (init) ex.callerOfNode.set(init, s.id);
      }
      ex.addSym(s, p);
      seen.set(name, s);
      ex.idOfNode.set(m, s.id);
      ex.queueDecl(m, sf, s);
    }
  }

  /** Reads `module.exports = value`: a function or class becomes a symbol of its own; an object literal exports each of its properties. */
  private moduleExports(a: ts.BinaryExpression, value: ts.Expression) {
    const at = statementOf(a);
    if (ts.isObjectLiteralExpression(value)) {
      for (const m of value.properties) {
        const name = ts.isSpreadAssignment(m) ? undefined : propName(m.name);
        if (name === undefined || ts.isSpreadAssignment(m) || ts.isShorthandPropertyAssignment(m) || ts.isAccessor(m))
          continue;
        this.namedExport(name, m, m, ts.isMethodDeclaration(m) ? m : m.initializer);
      }
      return;
    }
    const own = isFunctionValue(value) || ts.isClassExpression(value) ? value.name?.text : undefined;
    this.namedExport(own ?? "default", at, a.left, value);
  }

  /**
   * Declares one CommonJS export: `exports.name = value`, or a property of
   * `module.exports = { … }`. A name or a require() that it re-exports adds
   * no symbol: the declaration it names counts as exported.
   */
  private namedExport(name: string, at: ts.Node, left: ts.Node, raw: ts.Expression | ts.MethodDeclaration) {
    const ex = this.ex;
    const sf = this.sf;
    const value = ts.isMethodDeclaration(raw) ? raw : unwrap(raw);
    if (!ts.isMethodDeclaration(value) && (ts.isIdentifier(value) || requireSpec(value) !== undefined)) return;
    const base = {
      id: ex.symId(sf, name),
      name,
      parent: this.fileId,
      exported: true,
      location: ex.loc(sf, at),
    };
    let s: S.Symbol;
    if (ts.isMethodDeclaration(value) || isFunctionValue(value)) {
      const arrow = ts.isArrowFunction(value);
      s = ex.addSym(
        {
          ...base,
          kind: "function",
          languageKind: arrow ? "arrow function" : "function",
          signature: ex.signature(
            sf,
            value,
            ts.isMethodDeclaration(value) ? "method" : arrow ? "arrow" : "function",
            name,
          ),
        },
        name,
      );
      // An arrow function shows its assignment as the code writes it.
      if (arrow && s.signature?.text && !ts.isPropertyAssignment(at))
        s.signature.text = s.signature.text.replace(/^const \S+ = /, `${collapse(left.getText(sf))} = `);
      const mods = ex.modifiers(value);
      if (mods.length) s.modifiers = mods;
      ex.idOfNode.set(value, s.id);
      ex.callerOfNode.set(value, s.id);
    } else if (ts.isClassExpression(value)) {
      s = ex.addSym(ex.typeSymbol(sf, value, name, name, this.fileId, "class", "class", true), name);
      s.location = ex.loc(sf, at);
      ex.idOfNode.set(value, s.id);
      ex.members(sf, s, name, value.members);
      const ctor = value.members.find(m => ts.isConstructorDeclaration(m) && m.body);
      if (ctor) this.thisMembers(s.id, ctor as ts.ConstructorDeclaration);
    } else if (ts.isObjectLiteralExpression(value) && holdsFunction(value)) {
      s = ex.addSym({ ...base, kind: "namespace", languageKind: "object" }, name);
      ex.idOfNode.set(value, s.id);
      this.objectMembers(s, name, value, false);
    } else {
      s = ex.addSym(
        {
          ...base,
          kind: "variable",
          languageKind: "export",
          group: `${path.posix.basename(ex.rel(sf.fileName))}:${base.location.line}`,
          type: ex.typeOf(sf, left, undefined),
          value: shorten(collapse(value.getText(sf))),
        },
        name,
      );
      ex.callerOfNode.set(value, s.id);
    }
    ex.idOfNode.set(left, s.id);
    if (ts.isBinaryExpression(left.parent)) ex.idOfNode.set(left.parent, s.id);
    ex.queueDecl(at, sf, s);
  }

  /** Declares each @typedef and @callback in the comments of the file's top level. */
  private typedefs() {
    const seen = new Set<ts.JSDoc>();
    for (const node of [...this.sf.statements, this.sf.endOfFileToken]) {
      // Every JSDoc comment before a statement attaches to it, a typedef's too.
      for (const jd of (node as { jsDoc?: ts.JSDoc[] }).jsDoc ?? []) {
        if (seen.has(jd)) continue;
        seen.add(jd);
        let first = true;
        for (const tag of jd.tags ?? []) {
          if (!ts.isJSDocTypedefTag(tag) && !ts.isJSDocCallbackTag(tag)) continue;
          this.typedef(tag, first ? jd : undefined);
          first = false;
        }
      }
    }
  }

  /** Declares one @typedef or @callback. The first one of a comment takes the comment's text as its doc when the tag has no text of its own. */
  private typedef(tag: ts.JSDocTypedefTag | ts.JSDocCallbackTag, jd: ts.JSDoc | undefined) {
    const ex = this.ex;
    const sf = this.sf;
    const name = tag.name && ts.isIdentifier(tag.name) ? tag.name.text : undefined;
    if (!name) return;
    // @typedef {import("./x").Y} Y imports a type, as @import does; it declares nothing new.
    const it = tag.typeExpression && ts.isJSDocTypeExpression(tag.typeExpression) ? tag.typeExpression.type : undefined;
    if (it && ts.isImportTypeNode(it) && it.qualifier && ts.isIdentifier(it.qualifier) && it.qualifier.text === name) {
      const q = it.qualifier;
      ex.afterDeclarations(() => {
        const id = ex.idOfSymbol(ex.checker.getSymbolAtLocation(q));
        if (id) ex.idOfNode.set(tag, id);
      });
      return;
    }
    const s = ex.addSym(
      {
        id: ex.symId(sf, name),
        kind: "type_alias",
        languageKind: ts.isJSDocCallbackTag(tag) ? "callback" : "typedef",
        name,
        parent: this.fileId,
        exported: true,
        location: ex.loc(sf, tag),
      },
      name,
    );
    ex.idOfNode.set(tag, s.id);
    const te = tag.typeExpression;
    if (ts.isJSDocCallbackTag(tag)) this.callback(s, tag.typeExpression);
    else if (te && ts.isJSDocTypeLiteral(te)) {
      const parts: string[] = [];
      for (const pt of te.jsDocPropertyTags ?? []) {
        if (!ts.isIdentifier(pt.name)) continue;
        const pn = pt.name.text;
        const t = ex.typeOf(sf, pt, pt.typeExpression?.type);
        const optional = pt.isBracketed || (!!pt.typeExpression && ts.isJSDocOptionalType(pt.typeExpression.type));
        parts.push(`${pn}${optional ? "?" : ""}: ${t.text}`);
        const m = ex.addSym(
          {
            id: ex.symId(sf, `${name}.${pn}`),
            kind: "property",
            languageKind: "property",
            name: pn,
            parent: s.id,
            exported: true,
            location: ex.loc(sf, pt, false),
            type: t,
          },
          `${name}.${pn}`,
        );
        if (optional) m.modifiers = ["optional"];
        ex.idOfNode.set(pt, m.id);
        const text = ts.getTextOfJSDocComment(pt.comment)?.replace(/^\s*-\s*/, "");
        if (text) ex.queue(text, sf, pt, d => (m.doc = d));
      }
      s.type = { text: shorten(`{ ${parts.join("; ")} }`, 120), refs: [] };
    } else if (te && ts.isJSDocTypeExpression(te)) s.type = ex.typeOf(sf, tag, te.type);
    const text = ts.getTextOfJSDocComment(tag.comment)?.trim() || (jd && ts.getTextOfJSDocComment(jd.comment)?.trim());
    if (text) ex.queue(text, sf, tag, d => (s.doc = d));
  }

  /** Gives a @callback its signature and type, from its @param and @returns tags. */
  private callback(s: S.Symbol, sig: ts.JSDocSignature) {
    const ex = this.ex;
    const sf = this.sf;
    const params: S.Param[] = [];
    for (const pt of sig.parameters) {
      if (!ts.isIdentifier(pt.name)) continue;
      const t = ex.typeOf(sf, pt, pt.typeExpression?.type);
      const p: S.Param = { name: pt.name.text, type: t.text, refs: t.refs };
      if (pt.isBracketed) p.optional = true;
      params.push(p);
      const text = ts.getTextOfJSDocComment(pt.comment)?.replace(/^\s*-\s*/, "");
      if (text) ex.queueParamDoc(text, sf, pt, s, pt.name.text);
    }
    const results: S.Param[] = [];
    const rt = sig.type?.typeExpression?.type;
    if (rt && rt.kind !== ts.SyntaxKind.VoidKeyword) {
      const t = ex.typeOf(sf, sig.type!, rt);
      results.push({ type: t.text, refs: t.refs });
    }
    const list = params.map(p => p.name).join(", ");
    s.signature = { params, results, text: `@callback ${s.name}(${list})` };
    const ps = params.map(p => `${p.name}${p.optional ? "?" : ""}: ${p.type}`).join(", ");
    s.type = { text: shorten(`(${ps}) => ${results[0]?.type ?? "void"}`, 120), refs: [] };
  }
}
