// Text: the inline subset of docs/schema.md, doc blocks, and source links.

import type { Doc, Project } from "./types";

/** Escapes text for HTML. Every text from the data passes through it before it reaches the page. */
export function esc(s: unknown): string {
  return String(s ?? "").replace(
    /[&<>"']/g,
    c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!,
  );
}

/** What the text functions need from the map. */
interface Env {
  /** The project: its links, its repository and its autolinks. */
  project: Project;
  /** The commit that source links point at. */
  commit: string;
  /** Reports whether the map holds an ID, so a sym: link to nothing stays plain text. */
  knows: (id: string) => boolean;
}

/** The environment of the map on show. */
let env: Env = { project: { name: "" }, commit: "HEAD", knows: () => false };
/** The project's autolink rules, compiled. */
let autolinks: { re: RegExp; url: string }[] = [];

/** Sets the environment once the data has loaded, and compiles the autolinks; a pattern that does not parse is skipped with a warning. */
export function setTextEnv(e: Env) {
  env = e;
  autolinks = [];
  for (const a of e.project.autolinks ?? []) {
    try {
      autolinks.push({ re: new RegExp(a.pattern, "g"), url: a.url });
    } catch {
      console.warn("atlas: an autolink pattern does not parse:", a.pattern);
    }
  }
}

/** Fills a template's {placeholders} from vars; an unknown placeholder stays as written. */
function fill(template: string, vars: Record<string, string | number | undefined>): string {
  return template.replace(/\{(\w+)\}/g, (m, k) => (vars[k] !== undefined ? String(vars[k]) : m));
}

/**
 * Returns a URL only when it starts with http: or https:, else null. The
 * link templates and autolinks come from atlas.yaml, which a cloned
 * repository can write, so a javascript: or data: URL never reaches a link.
 */
export function webURL(u: string | null | undefined): string | null {
  return u && /^https?:\/\//i.test(u.trim()) ? u : null;
}

/** A URL for a file, a line or a range, from the project's link templates. */
export function sourceURL(path: string, line?: number, endLine?: number): string | null {
  const l = env.project.links;
  if (!l) return null;
  const vars = { repo: env.project.repo, commit: env.commit, path, line, endLine };
  if (line && endLine && endLine > line && l.range) return webURL(fill(l.range, vars));
  if (line && l.line) return webURL(fill(l.line, vars));
  return l.file ? webURL(fill(l.file, vars)) : null;
}

/** A URL for a folder, from the project's folder template, or null. */
export function folderURL(path: string): string | null {
  const l = env.project.links;
  if (!l?.folder) return null;
  return webURL(fill(l.folder, { repo: env.project.repo, commit: env.commit, path }));
}

/** An anchor to source, or plain text when the project has no links. */
export function sourceLink(label: string, path: string, line?: number, endLine?: number): string {
  const u = sourceURL(path, line, endLine);
  return u ? `<a href="${esc(u)}" target="_blank" rel="noopener">${esc(label)}</a>` : `<span>${esc(label)}</span>`;
}

/** Escapes plain text and turns the matches of the project's autolink rules into links. */
function autolinkPlain(text: string): string {
  if (!autolinks.length || !text) return esc(text);
  let out = "",
    pos = 0;
  for (;;) {
    let best: { i: number; m: RegExpExecArray; url: string } | null = null;
    for (const a of autolinks) {
      a.re.lastIndex = pos;
      const m = a.re.exec(text);
      if (m && m[0] && (!best || m.index < best.i)) best = { i: m.index, m, url: a.url };
    }
    if (!best) break;
    const { m } = best;
    const url = best.url
      .replace(/\$(\d)/g, (_, d) => m[Number(d)] ?? "")
      .replace(/\{(repo|commit)\}/g, (_, k) => (k === "repo" ? (env.project.repo ?? "") : env.commit));
    out +=
      esc(text.slice(pos, best.i)) +
      (webURL(url) ? `<a href="${esc(url)}" target="_blank" rel="noopener">${esc(m[0])}</a>` : esc(m[0]));
    pos = best.i + m[0].length;
  }
  return out + esc(text.slice(pos));
}

/** A link of the inline subset, found in a text. */
interface LinkMatch {
  /** The label between the brackets. */
  label: string;
  /** The target between the parentheses: a URL or "sym:<id>". */
  target: string;
  /** The position just after the link. */
  end: number;
}

/** Finds the link "[label](target)" that starts at position i, with nested brackets in its label, or null. */
function matchLink(s: string, i: number): LinkMatch | null {
  let depth = 0,
    j = i;
  for (; j < s.length; j++) {
    const c = s[j];
    if (c === "\\") {
      j++;
      continue;
    }
    if (c === "[") depth++;
    else if (c === "]" && --depth === 0) break;
  }
  if (j >= s.length || s[j + 1] !== "(") return null;
  const close = s.indexOf(")", j + 2);
  if (close < 0) return null;
  return { label: s.slice(i + 1, j), target: s.slice(j + 2, close), end: close + 1 };
}

/** Renders inline text as HTML. Symbol links become buttons with data-sym. */
export function inlineHTML(s: string | undefined): string {
  if (!s) return "";
  let out = "",
    plain = "",
    i = 0;
  const flush = () => {
    out += autolinkPlain(plain);
    plain = "";
  };
  while (i < s.length) {
    const c = s[i];
    if (c === "\\" && i + 1 < s.length) {
      plain += s[i + 1];
      i += 2;
      continue;
    }
    if (c === "`") {
      const j = s.indexOf("`", i + 1);
      if (j > i) {
        flush();
        out += `<code>${esc(s.slice(i + 1, j))}</code>`;
        i = j + 1;
        continue;
      }
    }
    if (c === "[") {
      const m = matchLink(s, i);
      if (m) {
        flush();
        const label = inlineHTML(m.label);
        if (m.target.startsWith("sym:")) {
          const id = m.target.slice(4);
          out += env.knows(id) ? `<button class="linkbtn" data-sym="${esc(id)}">${label}</button>` : label;
        } else if (/^https?:\/\//.test(m.target)) {
          out += `<a href="${esc(m.target)}" target="_blank" rel="noopener">${label}</a>`;
        } else out += label;
        i = m.end;
        continue;
      }
    }
    if (c === "*") {
      const strong = s[i + 1] === "*";
      const mark = strong ? "**" : "*";
      const j = s.indexOf(mark, i + mark.length);
      if (j > i + mark.length - 1 && s[i + mark.length] !== " ") {
        flush();
        const inner = inlineHTML(s.slice(i + mark.length, j));
        out += strong ? `<strong>${inner}</strong>` : `<em>${inner}</em>`;
        i = j + mark.length;
        continue;
      }
    }
    plain += c;
    i++;
  }
  flush();
  return out;
}

/** Strips the inline markup and returns plain text (for the canvas). */
export function inlinePlain(s: string | undefined): string {
  if (!s) return "";
  let out = "",
    i = 0;
  while (i < s.length) {
    const c = s[i];
    if (c === "\\" && i + 1 < s.length) {
      out += s[i + 1];
      i += 2;
      continue;
    }
    if (c === "`") {
      const j = s.indexOf("`", i + 1);
      if (j > i) {
        out += s.slice(i + 1, j);
        i = j + 1;
        continue;
      }
    }
    if (c === "[") {
      const m = matchLink(s, i);
      if (m) {
        out += inlinePlain(m.label);
        i = m.end;
        continue;
      }
    }
    if (c === "*") {
      i++;
      continue;
    }
    out += c;
    i++;
  }
  return out.replace(/\s+/g, " ").trim();
}

/** Returns a doc's first sentence as plain text: what every box shows under its name. */
export function summary(doc: Doc | undefined): string {
  return inlinePlain(doc?.summary);
}

/** The first paragraph, as plain text. */
export function firstPara(doc: Doc | undefined): string {
  const p = doc?.description.find(b => b.type === "paragraph");
  return p && p.type === "paragraph" ? inlinePlain(p.text) : summary(doc);
}

/** Splits text at blank lines and joins each paragraph's lines. */
export function plainParagraphs(text: string | undefined): string[] {
  return (text ?? "")
    .split(/\n\s*\n/)
    .map(p => p.replace(/\s*\n\s*/g, " ").trim())
    .filter(Boolean);
}

/** Renders paragraphs of inline text, such as a region's description, as HTML. */
export function paragraphsHTML(text: string | undefined): string {
  return plainParagraphs(text)
    .map(p => `<p>${inlineHTML(p)}</p>`)
    .join("");
}

/** Renders a doc in normal form. `none` names what has no doc. */
export function docHTML(doc: Doc | undefined, none = "No doc comment in the source."): string {
  if (!doc) return `<p class="nodoc">${esc(none)}</p>`;
  let h = "";
  if (doc.deprecated !== undefined)
    h += `<div class="deprecated"><b>Deprecated.</b> ${inlineHTML(doc.deprecated)}</div>`;
  for (const b of doc.description) {
    if (b.type === "paragraph") h += `<p>${inlineHTML(b.text)}</p>`;
    else if (b.type === "heading") h += `<h4>${inlineHTML(b.text)}</h4>`;
    else if (b.type === "quote") h += `<blockquote>${inlineHTML(b.text)}</blockquote>`;
    else if (b.type === "code") h += `<pre><code>${esc(b.code)}</code></pre>`;
    else if (b.type === "list") {
      const tag = b.ordered ? "ol" : "ul";
      h += `<${tag}>${b.items.map(i => `<li>${inlineHTML(i)}</li>`).join("")}</${tag}>`;
    }
  }
  const dl = (title: string, rows: [string, string][]) =>
    rows.length
      ? `<h4>${esc(title)}</h4><dl class="doctags">${rows.map(([k, v]) => `<dt><code>${esc(k)}</code></dt><dd>${inlineHTML(v)}</dd>`).join("")}</dl>`
      : "";
  h += dl("Type parameters", Object.entries(doc.typeParams ?? {}));
  h += dl("Parameters", Object.entries(doc.params ?? {}));
  if (doc.returns) h += `<h4>Returns</h4><p>${inlineHTML(doc.returns)}</p>`;
  if (doc.throws?.length)
    h += `<h4>Throws</h4><ul>${doc.throws.map(t => `<li>${t.type ? `<code>${esc(t.type)}</code> ` : ""}${inlineHTML(t.text)}</li>`).join("")}</ul>`;
  for (const ex of doc.examples ?? [])
    h += `<h4>${esc(ex.title || "Example")}</h4><pre><code>${esc(ex.code)}</code></pre>`;
  if (doc.seeAlso?.length)
    h += `<h4>See also</h4><ul>${doc.seeAlso.map(x => `<li>${inlineHTML(x)}</li>`).join("")}</ul>`;
  if (doc.since) h += `<p class="note">Since ${esc(doc.since)}.</p>`;
  for (const t of doc.tags ?? []) h += `<p class="note"><b>${esc(t.name)}</b> ${inlineHTML(t.text)}</p>`;
  return `<div class="doc">${h}</div>`;
}

/** Writes the "Machine-drafted" label for text that carries a provenance mark, or "" for text that a person wrote or checked. */
export function draftedNote(p: { by: string; at: string } | undefined): string {
  if (!p) return "";
  return `<p class="drafted" role="note"><b>Machine-drafted.</b> ${esc(p.by)} wrote this text on ${esc(p.at.slice(0, 10))}. No person has checked it yet.</p>`;
}
