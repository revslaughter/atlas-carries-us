// Smoke test for a built site. It serves the folder, opens each view in
// Chromium (dark and light), takes screenshots, and fails on console errors.
//
//   node tests/e2e/smoke.mjs <site folder> [screenshot prefix] [--light]
//
// The exit code is 1 when the page logs an error, a view fails to open, or a
// check below fails.

import { chromium } from "playwright";
import { createReadStream, existsSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, resolve } from "node:path";

const root = resolve(process.argv[2] ?? "site");
const prefix = process.argv[3] && !process.argv[3].startsWith("--") ? process.argv[3] : null;
const light = process.argv.includes("--light");
const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".json": "application/json", ".svg": "image/svg+xml", ".woff2": "font/woff2" };

const server = createServer((req, res) => {
  const p = decodeURIComponent((req.url ?? "/").split("?")[0]);
  const f = join(root, p === "/" ? "index.html" : p);
  if (!f.startsWith(root) || !existsSync(f) || !statSync(f).isFile()) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { "content-type": types[extname(f)] ?? "application/octet-stream" });
  createReadStream(f).pipe(res);
});
await new Promise(r => server.listen(0, "127.0.0.1", r));
const url = `http://127.0.0.1:${server.address().port}/`;

const failures = [];
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
page.on("console", m => { if (m.type() === "error") failures.push("console: " + m.text()); });
page.on("pageerror", e => failures.push("page error: " + e));
await page.emulateMedia({ colorScheme: light ? "light" : "dark", reducedMotion: "reduce" });
await page.goto(url);
await page.waitForFunction(() => window.atlas, null, { timeout: 15000 });
await page.waitForTimeout(400);

const shot = async name => { if (prefix) await page.screenshot({ path: `${prefix}-${name}.png` }); };
const check = (ok, what) => { if (!ok) failures.push("check: " + what); };

// Pick targets from the data, so the test works for any project.
const pick = await page.evaluate(() => {
  const M = window.atlas.model;
  const units = [...M.units.values()].sort((a, b) => b.items.length - a.items.length);
  const items = [...M.items.values()].filter(i => i.kind !== "values");
  const score = i => (M.out.get(i.id)?.length ?? 0) + (M.inc.get(i.id)?.length ?? 0);
  items.sort((a, b) => score(b) - score(a));
  const caller = [...M.mc.keys()].map(k => k.split("\0")).find(([a, m]) => m && (M.mc.get(a + "\0" + m)?.length ?? 0) >= 2);
  return { unit: units[0]?.id, item: items[0]?.id, entry: M.routes[0]?.id, trace: caller, tour: M.doc.tour?.length ?? 0 };
});

const views = [
  ["overview", null],
  ["overview-zoomed", () => { const a = window.atlas; a.go({ v: "overview" }); const c = a.debug.cam; a.debug.cam = { k: 0.6, x: c.x - 150, y: c.y - 80 }; }],
  ["unit", pick.unit && (u => window.atlas.go({ v: "unit", id: u })), pick.unit],
  ["symbol", pick.item && (i => window.atlas.go({ v: "symbol", id: i })), pick.item],
  ["trace", (pick.entry || pick.trace) && (([e, t]) => window.atlas.go(e ? { v: "trace", entry: e } : { v: "trace", sym: t[0], member: t[1] })), [pick.entry, pick.trace]],
  ["tour", pick.tour > 1 && (() => window.atlas.tour.go(1))],
];
for (const [name, fn, arg] of views) {
  if (fn === undefined || fn === false || fn === null) { if (name !== "overview") console.log(`skip ${name}: the data has no target for it`); else await shot(name); continue; }
  await page.evaluate(fn, arg);
  await page.waitForTimeout(500);
  await shot(name);
  const t = await page.evaluate(() => window.atlas.debug.scene?.t);
  check(t !== undefined, `${name}: a scene exists`);
}
if (pick.tour) await page.evaluate(() => window.atlas.tour.end());

// The panel shows the docs of the current view.
if (pick.item) {
  await page.evaluate(i => window.atlas.go({ v: "symbol", id: i }), pick.item);
  await page.waitForTimeout(300);
  check(await page.locator("#panel h2").count() > 0, "the symbol panel has a heading");
  // Stable links: the hash names the symbol by ID, and reloading reopens it.
  const hash = await page.evaluate(() => location.hash);
  check(hash.startsWith("#symbol="), "the hash names the symbol");
  await page.reload();
  await page.waitForFunction(() => window.atlas, null, { timeout: 15000 });
  await page.waitForTimeout(300);
  const again = await page.evaluate(() => { const s = window.atlas.debug.scene; return s?.t + ":" + s?.id; });
  check(again === "symbol:" + pick.item, `reload keeps the view (got ${again})`);
}

// Hovering a neighbour in the unit view draws its exact lines.
if (pick.unit) {
  await page.evaluate(u => window.atlas.go({ v: "unit", id: u }), pick.unit);
  await page.waitForTimeout(400);
  const box = await page.evaluate(() => {
    const a = window.atlas.debug, n = a.scene.nodes.find(n => n.t === "nbr");
    return n ? { x: n.x * a.cam.k + a.cam.x + 40, y: n.y * a.cam.k + a.cam.y + 20 } : null;
  });
  if (box) {
    const bb = await page.locator("#map").boundingBox();
    await page.mouse.move(bb.x + box.x, bb.y + box.y);
    await page.waitForTimeout(250);
    await shot("unit-hover");
  }
}

// Keyboard: Tab to the map, arrows move, Enter opens.
await page.evaluate(() => window.atlas.go({ v: "overview" }));
await page.focus("#map");
await page.keyboard.press("ArrowRight");
await page.waitForTimeout(150);
const said = await page.locator("#announce").textContent();
check((said ?? "").length > 0, "the map announces the keyboard selection");
await page.keyboard.press("Enter");
await page.waitForTimeout(500);
await shot("keyboard");

// The theme switch works and the page still draws.
await page.click("#themeBtn");
await page.waitForTimeout(300);
await shot("theme-switched");

await browser.close();
server.close();
if (failures.length) {
  console.error("smoke test failed:\n  " + failures.join("\n  "));
  process.exit(1);
}
console.log(`smoke test passed (${light ? "light" : "dark"} theme)`);
