// Doc comments: JSDoc and TSDoc text in the normal form of docs/schema.md,
// section 8. The text of a comment holds Markdown and block tags; this file
// splits the tags off, turns the Markdown into blocks, and writes inline text
// in the inline subset. A resolver that the caller passes in turns {@link}
// targets into symbol IDs.

import type { Block, Doc } from "./schema.js";

/** Turns the target of a link ("Store.get", "https://…") into a link target ("sym:<id>" or a URL), or undefined. */
export type LinkResolver = (target: string) => string | undefined;

/** Tags that only TSDoc defines. A comment that uses one counts as TSDoc. */
const TSDOC_TAGS = new Set([
  "remarks",
  "privateRemarks",
  "typeParam",
  "defaultValue",
  "packageDocumentation",
  "label",
  "decorator",
  "eventProperty",
  "sealed",
  "virtual",
]);

/** Tags that only describe types or structure, which the data already holds. */
const DROPPED_TAGS = new Set([
  "type",
  "typedef",
  "callback",
  "this",
  "augments",
  "extends",
  "implements",
  "satisfies",
  "overload",
  "import",
  "class",
  "constructor",
  "function",
  "func",
  "method",
  "member",
  "memberof",
  "name",
  "privateRemarks",
  "packageDocumentation",
  "module",
  "file",
  "fileoverview",
  "overview",
]);

/** Tags that mark a comment as the doc of its file rather than of the next declaration. */
export const FILE_TAGS = /(^|\n)\s*@(file|fileoverview|overview|module|packageDocumentation)\b/;

/** Removes the comment markers from a block or line comment, and the leading star of each line. */
export function commentText(comment: string): string {
  let t = comment;
  if (t.startsWith("//")) {
    return t
      .split("\n")
      .map(l => l.replace(/^\s*\/\/\/?\s?/, ""))
      .join("\n")
      .trim();
  }
  t = t.replace(/^\/\*\*?/, "").replace(/\*\/$/, "");
  const lines = t.split("\n").map(l => l.replace(/^\s*\*(?!\/) ?/, "").replace(/\s+$/, ""));
  while (lines.length && !lines[0].trim()) lines.shift();
  while (lines.length && !lines[lines.length - 1].trim()) lines.pop();
  return dedent(lines.join("\n"));
}

/** Removes the indentation that every non-blank line shares. */
export function dedent(s: string): string {
  const lines = s.split("\n");
  const ind = Math.min(...lines.filter(l => l.trim()).map(l => l.match(/^[ \t]*/)![0].length));
  if (!isFinite(ind) || ind === 0) return s;
  return lines.map(l => l.slice(ind)).join("\n");
}

/** One block tag as written, before its own rules apply. */
interface RawTag {
  /** The tag's name, without its "@". */
  name: string;
  /** Everything after the name, up to the next tag, with its lines. */
  text: string;
}

/** Splits a comment into its body and its block tags. Lines inside a code fence never start a tag. */
function splitTags(raw: string): { body: string; tags: RawTag[] } {
  const body: string[] = [];
  const tags: RawTag[] = [];
  let fence = "";
  for (const line of raw.split("\n")) {
    const f = line.match(/^\s*(```|~~~)/);
    const m = !fence && line.match(/^\s*@([A-Za-z][\w-]*)(.*)$/);
    if (m) {
      tags.push({ name: m[1], text: m[2].replace(/^\s/, "") });
    } else if (tags.length) {
      tags[tags.length - 1].text += "\n" + line;
    } else {
      body.push(line);
    }
    if (f) fence = fence ? (f[1] === fence ? "" : fence) : f[1];
  }
  for (const t of tags) t.text = t.text.replace(/\s+$/, "");
  return { body: body.join("\n").trim(), tags };
}

/** The characters that the inline subset gives a meaning to. */
const ESCAPE = /[\\`*[\]]/g;

/** Escapes the characters that the inline subset gives meaning to. */
export function escapeInline(s: string): string {
  return s.replace(ESCAPE, c => "\\" + c);
}

/** Encodes the characters that would end a link target early in the inline subset: parentheses and spaces. */
function escapeURL(u: string): string {
  return u.replace(/\(/g, "%28").replace(/\)/g, "%29").replace(/ /g, "%20");
}

/** Finds the end of a {@link …} tag that starts at i, or -1. */
function closeBrace(s: string, i: number): number {
  let depth = 0;
  for (let j = i; j < s.length; j++) {
    if (s[j] === "{") depth++;
    else if (s[j] === "}" && --depth === 0) return j;
  }
  return -1;
}

/** Writes one inline link tag ({@link}, {@linkcode}, {@linkplain}, {@inheritDoc}). */
function linkTag(kind: string, body: string, resolve: LinkResolver): string {
  let target = body.trim();
  let label = "";
  const bar = target.indexOf("|");
  if (bar >= 0) {
    label = target.slice(bar + 1).trim();
    target = target.slice(0, bar).trim();
  } else {
    const sp = target.search(/\s/);
    if (sp > 0) {
      label = target.slice(sp + 1).trim();
      target = target.slice(0, sp);
    }
  }
  const url = /^https?:\/\//.test(target) ? target : resolve(target.replace(/\(\)$/, ""));
  const shown = label
    ? inline(label, resolve)
    : kind === "linkcode"
      ? "`" + target.replace(/`/g, "'") + "`"
      : escapeInline(target);
  if (kind === "inheritDoc") return url ? `See [${shown}](${escapeURL(url)}).` : `See ${shown}.`;
  return url ? `[${shown}](${escapeURL(url)})` : shown;
}

/** Writes Markdown inline text in the inline subset. */
export function inline(md: string, resolve: LinkResolver): string {
  let out = "";
  let i = 0;
  const s = md.replace(/\s*\n\s*/g, " ");
  while (i < s.length) {
    const c = s[i];
    if (c === "\\" && i + 1 < s.length && /[\\`*_{}[\]()#+\-.!<>|~]/.test(s[i + 1])) {
      out += escapeInline(s[i + 1]);
      i += 2;
      continue;
    }
    if (c === "`") {
      const ticks = s.slice(i).match(/^`+/)![0];
      const j = s.indexOf(ticks, i + ticks.length);
      if (j > 0) {
        out +=
          "`" +
          s
            .slice(i + ticks.length, j)
            .trim()
            .replace(/`/g, "'") +
          "`";
        i = j + ticks.length;
        continue;
      }
    }
    if (c === "{" && s[i + 1] === "@") {
      const m = s.slice(i).match(/^\{@(link|linkcode|linkplain|inheritDoc)\b/);
      const end = closeBrace(s, i);
      if (m && end > 0) {
        out += linkTag(m[1], s.slice(i + m[0].length, end), resolve);
        i = end + 1;
        continue;
      }
    }
    if (c === "[") {
      const m = s.slice(i).match(/^\[((?:[^[\]\\]|\\.)*)\]\(([^()\s]+(?:\([^()\s]*\))?[^()\s]*)(?:\s+"[^"]*")?\)/);
      if (m) {
        out += `[${inline(m[1], resolve)}](${escapeURL(m[2])})`;
        i += m[0].length;
        continue;
      }
    }
    if (c === "<") {
      const m = s.slice(i).match(/^<(https?:\/\/[^>\s]+)>/);
      if (m) {
        out += `[${escapeInline(m[1])}](${escapeURL(m[1])})`;
        i += m[0].length;
        continue;
      }
    }
    if (c === "h" && (i === 0 || /[\s(]/.test(s[i - 1]))) {
      const m = s.slice(i).match(/^https?:\/\/[^\s<>()]+[^\s<>().,;:!?'"]/);
      if (m) {
        out += `[${escapeInline(m[0])}](${escapeURL(m[0])})`;
        i += m[0].length;
        continue;
      }
    }
    if (c === "*" || c === "_") {
      const strong = s[i + 1] === c;
      const mark = strong ? c + c : c;
      const wordStart = c === "*" || i === 0 || /[\s(]/.test(s[i - 1]);
      const next = s[i + mark.length];
      if (wordStart && next && next !== " " && next !== c) {
        const j = s.indexOf(mark, i + mark.length);
        const after = s[j + mark.length];
        if (j > i && s[j - 1] !== " " && (c === "*" || !after || /[\s).,;:!?]/.test(after))) {
          const inner = inline(s.slice(i + mark.length, j), resolve);
          out += strong ? `**${inner}**` : `*${inner}*`;
          i = j + mark.length;
          continue;
        }
      }
    }
    out += escapeInline(c);
    i++;
  }
  return out.trim();
}

/** Abbreviations whose period ends no sentence. */
const ABBREVIATIONS = /(?:^|\s)(?:e\.g|i\.e|etc|vs|cf|approx|Mr|Ms|Dr|No)\.$/i;

/** Returns the first sentence of a paragraph of Markdown. A sentence ends at ".", "!" or "?" before a space, outside code and links. */
export function firstSentence(md: string): string {
  const s = md.replace(/\s*\n\s*/g, " ").trim();
  let depth = 0;
  let code = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === "\\") {
      i++;
      continue;
    }
    if (c === "`") code = !code;
    if (code) continue;
    if (c === "[" || c === "{" || c === "(") depth++;
    else if ((c === "]" || c === "}" || c === ")") && depth > 0) depth--;
    else if (depth === 0 && (c === "." || c === "!" || c === "?") && (i + 1 === s.length || s[i + 1] === " ")) {
      const head = s.slice(0, i + 1);
      if (c === "." && (ABBREVIATIONS.test(head) || /(?:^|\s)[A-Z]\.$/.test(head))) continue;
      return head;
    }
  }
  return s;
}

/** Turns Markdown into blocks. */
export function blocks(md: string, resolve: LinkResolver): Block[] {
  const out: Block[] = [];
  const lines = md.split("\n");
  let para: string[] = [];
  let quote: string[] = [];
  let list: { ordered: boolean; items: string[] } | null = null;
  let lastBlank = true;
  const flush = () => {
    if (para.length) out.push({ type: "paragraph", text: inline(para.join(" "), resolve) });
    if (quote.length) out.push({ type: "quote", text: inline(quote.join(" "), resolve) });
    if (list) out.push({ type: "list", ordered: list.ordered, items: list.items.map(t => inline(t, resolve)) });
    para = [];
    quote = [];
    list = null;
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (!line.trim()) {
      if (para.length || quote.length) flush();
      lastBlank = true;
      continue;
    }
    const fence = line.match(/^(\s*)(```+|~~~+)\s*([\w+#.-]*)/);
    if (fence) {
      flush();
      const body: string[] = [];
      for (i++; i < lines.length && !lines[i].trim().startsWith(fence[2]); i++) body.push(lines[i]);
      const code = dedent(body.join("\n")).replace(/\s+$/, "");
      if (code) out.push(fence[3] ? { type: "code", code, language: fence[3] } : { type: "code", code });
      lastBlank = false;
      continue;
    }
    if (lastBlank && !list && /^( {4}|\t)/.test(line)) {
      flush();
      const body = [line];
      while (i + 1 < lines.length && (/^( {4}|\t)/.test(lines[i + 1]) || !lines[i + 1].trim())) body.push(lines[++i]);
      const code = dedent(body.join("\n")).replace(/\s+$/, "");
      out.push({ type: "code", code });
      lastBlank = false;
      continue;
    }
    const heading = line.match(/^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$/);
    if (heading) {
      flush();
      out.push({ type: "heading", level: heading[1].length, text: inline(heading[2], resolve) });
      lastBlank = false;
      continue;
    }
    const item = line.match(/^\s*([-*+]|\d+[.)])\s+(.*)$/);
    if (item) {
      if (para.length || quote.length) flush();
      const ordered = /\d/.test(item[1]);
      if (!list) list = { ordered, items: [] };
      list.items.push(item[2]);
      lastBlank = false;
      continue;
    }
    const q = line.match(/^\s*>\s?(.*)$/);
    if (q && !para.length && !list) {
      quote.push(q[1]);
      lastBlank = false;
      continue;
    }
    if (list && (!lastBlank || /^\s{2,}/.test(line))) {
      list.items[list.items.length - 1] += " " + line.trim();
      lastBlank = false;
      continue;
    }
    if (list) flush();
    para.push(line.trim());
    lastBlank = false;
  }
  flush();
  return out;
}

/** Parses one @example tag: an optional title, then code in a fence or as written. */
function example(text: string, language: string): { title?: string; code: string; language?: string } {
  let t = text.replace(/^\s*\n/, "");
  let title = "";
  const cap = t.match(/^\s*<caption>([\s\S]*?)<\/caption>\s*/);
  if (cap) {
    title = cap[1].trim();
    t = t.slice(cap[0].length);
  }
  const fence = t.match(/^([\s\S]*?)^[ \t]*(```+|~~~+)[ \t]*([\w+#.-]*)[^\n]*\n([\s\S]*?)^[ \t]*\2[ \t]*$/m);
  if (fence) {
    const head = fence[1].trim().replace(/\s*\n\s*/g, " ");
    const code = dedent(fence[4]).replace(/\s+$/, "");
    const ex: { title?: string; code: string; language?: string } = { code, language: fence[3] || language };
    if (head || title) ex.title = head || title;
    return ex;
  }
  const ex: { title?: string; code: string; language?: string } = { code: dedent(t).trim(), language };
  if (title) ex.title = title;
  return ex;
}

/** Splits "{Type} name - text" into its parts. The type and the name may be absent. */
function nameAndText(text: string): { type: string; name: string; text: string } {
  let t = text.trim();
  let type = "";
  if (t.startsWith("{")) {
    const end = closeBrace(t, 0);
    if (end > 0) {
      type = t.slice(1, end).trim();
      t = t.slice(end + 1).trim();
    }
  }
  let name = "";
  const m = t.match(/^(\[[^\]]*\]|[\w$.[\]]+)/);
  if (m) {
    name = m[1];
    t = t.slice(m[0].length);
  }
  if (name.startsWith("[")) name = name.slice(1, -1).split("=")[0].trim();
  t = t.replace(/^\s*-\s*/, "").trim();
  return { type, name, text: t };
}

/** Builds the normal form of one doc comment. `raw` holds the comment without its markers. */
export function normalise(raw: string, resolve: LinkResolver, codeLanguage: string): Doc | undefined {
  raw = raw.trim();
  if (!raw) return undefined;
  const { body, tags } = splitTags(raw);
  let text = body;
  const extra: string[] = [];
  const doc: Doc = { raw, format: "jsdoc", summary: "", description: [] };
  const params: Record<string, string> = {};
  const typeParams: Record<string, string> = {};
  for (const t of tags) {
    if (TSDOC_TAGS.has(t.name)) doc.format = "tsdoc";
    const one = () => inline(t.text, resolve);
    switch (t.name) {
      case "description":
      case "desc":
      case "file":
      case "fileoverview":
      case "overview":
        text = [text, t.text.trim()].filter(Boolean).join("\n\n");
        break;
      case "packageDocumentation":
      case "module": {
        // These tags mark a file doc. The lines after them hold its text; a
        // module name may follow @module on its own line.
        const nl = t.text.indexOf("\n");
        if (nl >= 0) text = [text, t.text.slice(nl + 1).trim()].filter(Boolean).join("\n\n");
        break;
      }
      case "remarks":
        extra.push(t.text.trim());
        break;
      case "param":
      case "arg":
      case "argument": {
        const p = nameAndText(t.text);
        if (p.name) params[p.name] = inline(p.text, resolve);
        break;
      }
      case "typeParam":
      case "template": {
        const m = t.text.replace(/^\s*\{[^}]*\}\s*/, "").match(/^\s*([\w$]+(?:\s*,\s*[\w$]+)*)\s*(?:-\s*)?([\s\S]*)$/);
        if (m) for (const n of m[1].split(",")) typeParams[n.trim()] = inline(m[2], resolve);
        break;
      }
      case "returns":
      case "return":
        doc.returns = inline(t.text.replace(/^\s*\{[^}]*\}\s*/, ""), resolve);
        break;
      case "throws":
      case "throw":
      case "exception": {
        const p = t.text.trim().startsWith("{") ? nameAndText(t.text) : { type: "", name: "", text: t.text };
        const rest = [p.name, p.text].filter(Boolean).join(" ");
        const th: { type?: string; text: string } = { text: inline(rest, resolve) };
        if (p.type) th.type = p.type;
        (doc.throws ??= []).push(th);
        break;
      }
      case "example":
        (doc.examples ??= []).push(example(t.text, codeLanguage));
        break;
      case "deprecated":
        doc.deprecated = one();
        break;
      case "since":
        doc.since = t.text.trim();
        break;
      case "see": {
        const target = t.text.trim();
        const bare = /^[\w$.#]+(\(\))?$/.test(target) ? resolve(target.replace(/\(\)$/, "")) : undefined;
        (doc.seeAlso ??= []).push(bare ? `[${escapeInline(target)}](${escapeURL(bare)})` : one());
        break;
      }
      case "defaultValue":
      case "default":
        (doc.tags ??= []).push({ name: "default", text: one() });
        break;
      default:
        if (!DROPPED_TAGS.has(t.name)) (doc.tags ??= []).push({ name: t.name, text: one() });
    }
  }
  if (/\{@inheritDoc\b/.test(raw)) doc.format = "tsdoc";
  doc.description = blocks([text, ...extra].filter(Boolean).join("\n\n"), resolve);
  const first = text.split(/\n\s*\n/).find(p => p.trim() && !/^\s*(#|```|~~~|[-*+]\s|\d+[.)]\s|>)/.test(p));
  doc.summary = first ? inline(firstSentence(first), resolve) : "";
  if (Object.keys(params).length) doc.params = params;
  if (Object.keys(typeParams).length) doc.typeParams = typeParams;
  if (
    !doc.description.length &&
    doc.summary === "" &&
    !Object.keys(doc).some(k => !["raw", "format", "summary", "description"].includes(k))
  )
    return undefined;
  return doc;
}
