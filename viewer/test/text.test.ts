import { test } from "node:test";
import assert from "node:assert/strict";
import { inlineHTML, inlinePlain, setTextEnv, sourceURL } from "../src/text";

setTextEnv({
  project: {
    name: "x",
    repo: "o/r",
    links: {
      line: "https://example.com/{repo}/{commit}/{path}#L{line}",
      file: "https://example.com/{repo}/{commit}/{path}",
    },
    autolinks: [{ pattern: "(?<![\\w/])#(\\d+)\\b", url: "https://example.com/{repo}/issues/$1" }],
  },
  commit: "abc",
  knows: id => id === "go:p#Known",
});

test("escapes HTML in plain text", () => {
  assert.equal(inlineHTML("a <b> & c"), "a &lt;b&gt; &amp; c");
});

test("renders code spans, emphasis and escapes", () => {
  assert.equal(
    inlineHTML("use `x < y` *now* **here** \\*not\\*"),
    "use <code>x &lt; y</code> <em>now</em> <strong>here</strong> *not*",
  );
});

test("links to known symbols and drops links to unknown ones", () => {
  assert.equal(inlineHTML("[Known](sym:go:p#Known)"), '<button class="linkbtn" data-sym="go:p#Known">Known</button>');
  assert.equal(inlineHTML("[Gone](sym:go:p#Gone)"), "Gone");
});

test("links web addresses and refuses other schemes", () => {
  assert.match(
    inlineHTML("[docs](https://example.com/a)"),
    /^<a href="https:\/\/example.com\/a" target="_blank" rel="noopener">docs<\/a>$/,
  );
  assert.doesNotMatch(inlineHTML("[x](javascript:alert(1))"), /href|<a/);
});

test("applies autolinks to plain text only", () => {
  assert.equal(
    inlineHTML("see #12 and `#13`"),
    'see <a href="https://example.com/o/r/issues/12" target="_blank" rel="noopener">#12</a> and <code>#13</code>',
  );
});

test("strips markup for the canvas", () => {
  assert.equal(inlinePlain("Read [Mux.Get](sym:go:p#Mux.Get) and `code` \\[x\\]"), "Read Mux.Get and code [x]");
});

test("fills link templates", () => {
  assert.equal(sourceURL("a/b.go", 7), "https://example.com/o/r/abc/a/b.go#L7");
  assert.equal(sourceURL("a/b.go"), "https://example.com/o/r/abc/a/b.go");
});
