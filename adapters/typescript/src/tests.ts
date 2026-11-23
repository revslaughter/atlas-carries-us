// The test index: which tests use each symbol. A test is a call to it(),
// test() or specify() with a title, inside any number of describe() blocks;
// its name joins the titles with " > ", as Vitest and Jest print them.

import ts from "typescript";

import type { Extractor } from "./extract.js";

const SUITES = new Set(["describe", "suite", "context", "fdescribe", "xdescribe"]);
const TESTS = new Set(["it", "test", "specify", "fit", "xit", "bench"]);

/** The name a call starts from: "describe" in describe.each([...])("title", fn). */
function baseName(e: ts.Expression): string | undefined {
  for (;;) {
    if (ts.isCallExpression(e) || ts.isPropertyAccessExpression(e)) e = e.expression;
    else if (ts.isTaggedTemplateExpression(e)) e = e.tag;
    else break;
  }
  return ts.isIdentifier(e) ? e.text : undefined;
}

function title(e: ts.Expression | undefined, sf: ts.SourceFile): string {
  if (!e) return "";
  if (ts.isStringLiteralLike(e)) return e.text;
  if (ts.isTemplateExpression(e)) return e.getText(sf).slice(1, -1);
  return e.getText(sf);
}

/** Records, for each symbol of the map, the tests whose bodies name it. */
export function indexTests(ex: Extractor, files: readonly ts.SourceFile[]) {
  for (const sf of files) {
    const path = ex.rel(sf.fileName);
    const uses = (body: ts.Node, name: string, line: number) => {
      const visit = (n: ts.Node) => {
        if (ts.isIdentifier(n) || ts.isPrivateIdentifier(n)) {
          const id = ex.idOfSymbol(ex.checker.getSymbolAtLocation(n));
          if (id) ex.addTestUse(id, { name, location: { path, line } });
        }
        ts.forEachChild(n, visit);
      };
      visit(body);
    };
    const visit = (n: ts.Node, suite: string[]) => {
      if (ts.isCallExpression(n)) {
        const base = baseName(n.expression);
        const fns = n.arguments.filter(a => ts.isArrowFunction(a) || ts.isFunctionExpression(a));
        if (base && SUITES.has(base) && fns.length) {
          const inner = [...suite, title(n.arguments[0], sf)];
          for (const f of fns) visit(f, inner);
          return;
        }
        if (base && TESTS.has(base) && fns.length) {
          const name = [...suite, title(n.arguments[0], sf)].join(" > ");
          for (const f of fns) uses(f, name, ex.line(n, sf));
          return;
        }
      }
      ts.forEachChild(n, c => visit(c, suite));
    };
    visit(sf, []);
  }
}
