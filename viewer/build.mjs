// Builds the viewer into dist/: app.js (one bundle), index.html, styles.css,
// fonts and the Ahro mark. `node build.mjs --tests` builds the unit tests
// into dist-test/ instead.

import { build } from "esbuild";
import { cpSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

if (process.argv.includes("--tests")) {
  const out = join(here, "dist-test");
  rmSync(out, { recursive: true, force: true });
  const entries = readdirSync(join(here, "test")).filter(f => f.endsWith(".test.ts")).map(f => join(here, "test", f));
  await build({ entryPoints: entries, outdir: out, bundle: true, platform: "node", format: "esm", outExtension: { ".js": ".mjs" }, target: "node20", logLevel: "warning" });
  process.exit(0);
}

const dist = join(here, "dist");
rmSync(dist, { recursive: true, force: true });
mkdirSync(join(dist, "fonts"), { recursive: true });
mkdirSync(join(dist, "assets"), { recursive: true });

await build({
  entryPoints: [join(here, "src/main.ts")],
  outfile: join(dist, "app.js"),
  bundle: true,
  minify: true,
  sourcemap: false,
  format: "iife",
  target: ["es2022", "chrome110", "firefox115", "safari16"],
  logLevel: "warning",
});

cpSync(join(here, "src/index.html"), join(dist, "index.html"));
cpSync(join(here, "src/styles.css"), join(dist, "styles.css"));
cpSync(join(here, "assets/ahro-mark-coin.svg"), join(dist, "assets/ahro-mark-coin.svg"));

const fonts = {
  "@fontsource/inter": ["inter-latin-400-normal.woff2", "inter-latin-500-normal.woff2", "inter-latin-600-normal.woff2", "inter-latin-700-normal.woff2"],
  "@fontsource/jetbrains-mono": ["jetbrains-mono-latin-400-normal.woff2", "jetbrains-mono-latin-600-normal.woff2", "jetbrains-mono-latin-700-normal.woff2"],
  "@fontsource-variable/fraunces": ["fraunces-latin-wght-normal.woff2", "fraunces-latin-wght-italic.woff2"],
};
let licences = "The fonts in this folder carry the SIL Open Font License, Version 1.1.\n";
for (const [pkg, files] of Object.entries(fonts)) {
  const root = dirname(require.resolve(pkg + "/package.json"));
  for (const f of files) cpSync(join(root, "files", f), join(dist, "fonts", f));
  licences += `\n==== ${pkg} ====\n\n` + readFileSync(join(root, "LICENSE"), "utf8");
}
writeFileSync(join(dist, "fonts", "LICENSES.txt"), licences);
console.log("viewer: built dist/");
