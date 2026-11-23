//! Doc comments: rustdoc Markdown in the normal form of `docs/schema.md`,
//! section 8. The text turns into blocks, and inline text into the inline
//! subset. A resolver that the caller passes in turns intra-doc links
//! (`[Store]`, `` [`Store::find`] ``, `[text](Store)`) into symbol IDs.
//!
//! The rules follow the TypeScript adapter's `docs.ts`, plus what rustdoc
//! adds: shortcut and reference links, code blocks that count as Rust unless
//! they say otherwise, and hidden lines (`# use …;`) in those blocks.

use std::collections::HashMap;

use crate::schema::{Block, Doc};

/// Turns a link target as written (`Store`, `` `Store::find` ``) into a link
/// target of the inline subset (`sym:<id>` or a URL). An empty string means
/// that rustdoc knows the target but no page holds it: the label stays, as
/// plain text. `None` means that the text names no link.
pub type Resolver<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The words of a code block's info string that rustdoc reads as test
/// settings, not as a language.
const TEST_WORDS: &[&str] = &[
    "rust",
    "ignore",
    "no_run",
    "should_panic",
    "compile_fail",
    "test_harness",
    "standalone_crate",
    "allow_fail",
];

/// Abbreviations whose period ends no sentence.
const ABBREVIATIONS: &[&str] = &[
    "e.g.", "i.e.", "etc.", "vs.", "cf.", "approx.", "Mr.", "Ms.", "Dr.", "No.",
];

/// Turns a rustdoc comment into the normal form. Returns `None` for an empty
/// comment on an item that rustdoc does not mark deprecated.
pub fn normalise(raw: &str, resolve: Resolver, deprecated: Option<String>) -> Option<Doc> {
    let raw = raw.trim();
    if raw.is_empty() && deprecated.is_none() {
        return None;
    }
    let (text, refs) = split_references(raw);
    let ctx = Context { resolve, refs };
    let description = ctx.blocks(&text);
    let summary = text
        .split("\n\n")
        .map(str::trim)
        .find(|p| !p.is_empty() && !starts_block(p))
        .map(|p| ctx.inline(&first_sentence(p)))
        .unwrap_or_default();
    Some(Doc {
        raw: raw.to_string(),
        format: "rustdoc".into(),
        summary,
        description,
        deprecated,
    })
}

/// Reports whether a paragraph starts a block other than running text: a
/// heading, a fence, a list or a quote.
fn starts_block(p: &str) -> bool {
    let t = p.trim_start();
    t.starts_with('#')
        || t.starts_with("```")
        || t.starts_with("~~~")
        || t.starts_with('>')
        || list_item(t).is_some()
}

/// Holds what inline text needs: the resolver and the reference definitions.
struct Context<'a> {
    /// Resolves intra-doc link targets.
    resolve: Resolver<'a>,
    /// Maps a reference label, in lower case, to its destination.
    refs: HashMap<String, String>,
}

/// Takes the reference definitions (`[label]: target`) out of a comment.
fn split_references(raw: &str) -> (String, HashMap<String, String>) {
    let mut refs = HashMap::new();
    let mut kept = Vec::new();
    let mut fence = false;
    for line in raw.lines() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
        }
        if !fence && t.starts_with('[') {
            if let Some(end) = t.find("]:") {
                let label = &t[1..end];
                let dest = t[end + 2..].split_whitespace().next().unwrap_or("");
                if !label.is_empty() && !dest.is_empty() && !label.contains(']') {
                    refs.insert(
                        label.to_lowercase(),
                        dest.trim_matches(['<', '>']).to_string(),
                    );
                    continue;
                }
            }
        }
        kept.push(line);
    }
    (kept.join("\n"), refs)
}

/// Escapes the characters that the inline subset gives meaning to.
pub fn escape_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '`' | '*' | '[' | ']') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Encodes the characters that would end a link target early.
fn escape_url(u: &str) -> String {
    u.replace('(', "%28")
        .replace(')', "%29")
        .replace(' ', "%20")
}

/// Reports whether a link destination names a web page or a mail address.
fn is_url(d: &str) -> bool {
    d.starts_with("http://") || d.starts_with("https://") || d.starts_with("mailto:")
}

/// Finds the character that closes the bracket at `open`, skipping code
/// spans and escapes. Returns its index.
fn close_bracket(s: &[char], open: usize, (o, c): (char, char)) -> Option<usize> {
    let mut depth = 0;
    let mut i = open;
    while i < s.len() {
        match s[i] {
            '\\' => i += 1,
            '`' => {
                let n = s[i..].iter().take_while(|&&x| x == '`').count();
                let mut j = i + n;
                while j < s.len() {
                    let m = s[j..].iter().take_while(|&&x| x == '`').count();
                    if m == n {
                        break;
                    }
                    j += m.max(1);
                }
                i = j + n - 1;
            }
            x if x == o => depth += 1,
            x if x == c => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

impl Context<'_> {
    /// Turns a link destination into a target of the inline subset.
    fn target(&self, dest: &str) -> Option<String> {
        if is_url(dest) {
            return Some(escape_url(dest));
        }
        (self.resolve)(dest)
            .filter(|t| !t.is_empty())
            .map(|t| escape_url(&t))
    }

    /// Writes one link: its label, and its target when one resolves.
    fn link(&self, label: &str, target: Option<String>) -> String {
        let shown = self.inline(label);
        match target {
            Some(t) => format!("[{shown}]({t})"),
            None => shown,
        }
    }

    /// Writes Markdown inline text in the inline subset.
    pub fn inline(&self, md: &str) -> String {
        let joined: String = md.split_whitespace().collect::<Vec<_>>().join(" ");
        let s: Vec<char> = joined.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < s.len() {
            let c = s[i];
            if c == '\\' && i + 1 < s.len() && "\\`*_{}[]()#+-.!<>|~".contains(s[i + 1]) {
                out.push_str(&escape_inline(&s[i + 1].to_string()));
                i += 2;
                continue;
            }
            if c == '`' {
                let n = s[i..].iter().take_while(|&&x| x == '`').count();
                let rest: String = s[i + n..].iter().collect();
                let ticks = "`".repeat(n);
                if let Some(j) = rest.find(&ticks) {
                    let code = rest[..j].trim().replace('`', "'");
                    out.push('`');
                    out.push_str(&code);
                    out.push('`');
                    i += n + rest[..j].chars().count() + n;
                    continue;
                }
            }
            if c == '[' {
                if let Some(end) = close_bracket(&s, i, ('[', ']')) {
                    let label: String = s[i + 1..end].iter().collect();
                    let next = s.get(end + 1).copied();
                    if next == Some('(') {
                        if let Some(close) = close_bracket(&s, end + 1, ('(', ')')) {
                            let inner: String = s[end + 2..close].iter().collect();
                            let dest = inner
                                .split_whitespace()
                                .next()
                                .unwrap_or("")
                                .trim_matches(['<', '>']);
                            out.push_str(&self.link(&label, self.target(dest)));
                            i = close + 1;
                            continue;
                        }
                    }
                    if next == Some('[') {
                        if let Some(close) = close_bracket(&s, end + 1, ('[', ']')) {
                            let r: String = s[end + 2..close].iter().collect();
                            let key = if r.is_empty() { label.clone() } else { r };
                            let target = match self.refs.get(&key.to_lowercase()) {
                                Some(d) => self.target(d),
                                None => self.target(&key),
                            };
                            out.push_str(&self.link(&label, target));
                            i = close + 1;
                            continue;
                        }
                    }
                    let target = match self.refs.get(&label.to_lowercase()) {
                        Some(d) => self.target(d).or(Some(String::new())),
                        None => (self.resolve)(&label).map(|t| escape_url(&t)),
                    };
                    if let Some(t) = target {
                        out.push_str(&self.link(&label, Some(t).filter(|t| !t.is_empty())));
                    } else {
                        out.push_str("\\[");
                        out.push_str(&self.inline(&label));
                        out.push_str("\\]");
                    }
                    i = end + 1;
                    continue;
                }
            }
            if c == '<' {
                let rest: String = s[i + 1..].iter().collect();
                if let Some(j) = rest.find('>') {
                    let u = &rest[..j];
                    if is_url(u) && !u.contains(char::is_whitespace) {
                        out.push_str(&format!("[{}]({})", escape_inline(u), escape_url(u)));
                        i += j + 2;
                        continue;
                    }
                }
            }
            if c == 'h' && (i == 0 || s[i - 1] == ' ' || s[i - 1] == '(') {
                let rest: String = s[i..].iter().collect();
                if rest.starts_with("http://") || rest.starts_with("https://") {
                    let mut u: String = rest
                        .chars()
                        .take_while(|c| !c.is_whitespace() && !"<>()".contains(*c))
                        .collect();
                    while u.ends_with(['.', ',', ';', ':', '!', '?', '\'', '"']) {
                        u.pop();
                    }
                    out.push_str(&format!("[{}]({})", escape_inline(&u), escape_url(&u)));
                    i += u.chars().count();
                    continue;
                }
            }
            if c == '*' || c == '_' {
                let strong = s.get(i + 1) == Some(&c);
                let n = if strong { 2 } else { 1 };
                let word_start = c == '*' || i == 0 || s[i - 1] == ' ' || s[i - 1] == '(';
                let next = s.get(i + n).copied();
                if word_start && next.is_some_and(|x| x != ' ' && x != c) {
                    let mark: String = std::iter::repeat_n(c, n).collect();
                    let rest: String = s[i + n..].iter().collect();
                    if let Some(j) = rest.find(&mark) {
                        let jc = rest[..j].chars().count();
                        let after = s.get(i + n + jc + n).copied();
                        let before = s[i + n + jc - 1];
                        if jc > 0
                            && before != ' '
                            && (c == '*' || after.is_none_or(|a| " ).,;:!?".contains(a)))
                        {
                            let inner = self.inline(&rest[..j]);
                            out.push_str(&if strong {
                                format!("**{inner}**")
                            } else {
                                format!("*{inner}*")
                            });
                            i += n + jc + n;
                            continue;
                        }
                    }
                }
            }
            out.push_str(&escape_inline(&c.to_string()));
            i += 1;
        }
        out.trim().to_string()
    }

    /// Turns Markdown into blocks.
    pub fn blocks(&self, md: &str) -> Vec<Block> {
        let mut b = Builder::default();
        let lines: Vec<&str> = md.lines().collect();
        let mut i = 0;
        let mut last_blank = true;
        while i < lines.len() {
            let line = lines[i];
            if line.trim().is_empty() {
                if !b.para.is_empty() || !b.quote.is_empty() {
                    b.flush(self);
                }
                last_blank = true;
                i += 1;
                continue;
            }
            let t = line.trim_start();
            let fence = ["```", "~~~"].into_iter().find(|f| t.starts_with(f));
            if let Some(f) = fence {
                b.flush(self);
                let info = t.trim_start_matches(f.chars().next().unwrap_or('`')).trim();
                let (language, rust) = fence_language(info);
                let mut body = Vec::new();
                i += 1;
                while i < lines.len() && !lines[i].trim_start().starts_with(f) {
                    body.push(lines[i]);
                    i += 1;
                }
                let code = code_text(&body, rust);
                if !code.is_empty() {
                    b.out.push(code_block(code, language));
                }
                last_blank = false;
                i += 1;
                continue;
            }
            if last_blank
                && b.list.is_none()
                && (line.starts_with("    ") || line.starts_with('\t'))
            {
                b.flush(self);
                let mut body = vec![line];
                while i + 1 < lines.len()
                    && (lines[i + 1].starts_with("    ")
                        || lines[i + 1].starts_with('\t')
                        || lines[i + 1].trim().is_empty())
                {
                    i += 1;
                    body.push(lines[i]);
                }
                let code = code_text(&body, true);
                if !code.is_empty() {
                    b.out.push(code_block(code, "rust".into()));
                }
                last_blank = false;
                i += 1;
                continue;
            }
            if let Some((level, text)) = heading(line) {
                b.flush(self);
                b.out.push(Block {
                    kind: "heading".into(),
                    level,
                    text: self.inline(text),
                    ..Block::default()
                });
                last_blank = false;
                i += 1;
                continue;
            }
            if let Some((ordered, text)) = list_item(t) {
                if !b.para.is_empty() || !b.quote.is_empty() {
                    b.flush(self);
                }
                b.list
                    .get_or_insert((ordered, Vec::new()))
                    .1
                    .push(text.to_string());
                last_blank = false;
                i += 1;
                continue;
            }
            if let Some(q) = t.strip_prefix('>') {
                if b.para.is_empty() && b.list.is_none() {
                    b.quote.push(q.trim().to_string());
                    last_blank = false;
                    i += 1;
                    continue;
                }
            }
            if let Some((_, items)) = b.list.as_mut() {
                if !last_blank || line.starts_with("  ") {
                    if let Some(last) = items.last_mut() {
                        last.push(' ');
                        last.push_str(line.trim());
                    }
                    last_blank = false;
                    i += 1;
                    continue;
                }
                b.flush(self);
            }
            b.para.push(line.trim().to_string());
            last_blank = false;
            i += 1;
        }
        b.flush(self);
        b.out
    }
}

/// Collects blocks while `blocks` reads the lines.
#[derive(Default)]
struct Builder {
    /// The blocks so far.
    out: Vec<Block>,
    /// The lines of the paragraph in progress.
    para: Vec<String>,
    /// The lines of the quote in progress.
    quote: Vec<String>,
    /// The list in progress: whether it counts, and its items.
    list: Option<(bool, Vec<String>)>,
}

impl Builder {
    /// Ends the paragraph, quote or list in progress.
    fn flush(&mut self, ctx: &Context) {
        if !self.para.is_empty() {
            self.out.push(Block {
                kind: "paragraph".into(),
                text: ctx.inline(&self.para.join(" ")),
                ..Block::default()
            });
        }
        if !self.quote.is_empty() {
            self.out.push(Block {
                kind: "quote".into(),
                text: ctx.inline(&self.quote.join(" ")),
                ..Block::default()
            });
        }
        if let Some((ordered, items)) = self.list.take() {
            self.out.push(Block {
                kind: "list".into(),
                ordered: Some(ordered),
                items: items.iter().map(|t| ctx.inline(t)).collect(),
                ..Block::default()
            });
        }
        self.para.clear();
        self.quote.clear();
    }
}

/// Makes a code block.
fn code_block(code: String, language: String) -> Block {
    Block {
        kind: "code".into(),
        code,
        language,
        ..Block::default()
    }
}

/// Reads a heading line: `# Errors` gives level 1 and "Errors".
fn heading(line: &str) -> Option<(u32, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let t = &line[indent..];
    let level = t.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &t[level..];
    if !rest.starts_with(' ') && !rest.is_empty() {
        return None;
    }
    Some((level as u32, rest.trim().trim_end_matches('#').trim()))
}

/// Reads a list item: `- text`, `* text`, `+ text` or `1. text`. Returns
/// whether the list counts, and the item's text.
fn list_item(t: &str) -> Option<(bool, &str)> {
    for m in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(m) {
            return Some((false, rest.trim()));
        }
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &t[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some((true, r.trim()));
        }
    }
    None
}

/// Reads a fence's info string. Returns the language, and whether the block
/// counts as Rust. A block without a language, or with only rustdoc's test
/// words, counts as Rust.
fn fence_language(info: &str) -> (String, bool) {
    for w in info.split([',', ' ', '\t']).filter(|w| !w.is_empty()) {
        let test = TEST_WORDS.contains(&w) || w.starts_with("edition") || w.starts_with("ignore-");
        if !test {
            return (w.to_string(), false);
        }
    }
    ("rust".into(), true)
}

/// Joins the lines of a code block, removes their common indent, and drops
/// the lines that rustdoc hides in a Rust block (`# use …;`).
fn code_text(lines: &[&str], rust: bool) -> String {
    let shown: Vec<String> = lines
        .iter()
        .filter_map(|l| {
            if !rust {
                return Some(l.to_string());
            }
            let t = l.trim_start();
            if t == "#" || t.starts_with("# ") {
                return None;
            }
            if let Some(rest) = t.strip_prefix("##") {
                let indent = &l[..l.len() - t.len()];
                return Some(format!("{indent}#{rest}"));
            }
            Some(l.to_string())
        })
        .collect();
    dedent(&shown.join("\n"))
        .trim_end()
        .trim_start_matches('\n')
        .to_string()
}

/// Removes the indent that every line of a text shares.
fn dedent(s: &str) -> String {
    let indent = s
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    s.lines()
        .map(|l| {
            if l.len() >= indent {
                &l[indent..]
            } else {
                l.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Returns the first sentence of a paragraph of Markdown. A sentence ends at
/// ".", "!" or "?" before a space, outside code and links.
pub fn first_sentence(md: &str) -> String {
    let s: String = md.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = s.chars().collect();
    let mut depth = 0;
    let mut code = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            i += 2;
            continue;
        }
        if c == '`' {
            code = !code;
        }
        if !code {
            if matches!(c, '[' | '{' | '(') {
                depth += 1;
            } else if matches!(c, ']' | '}' | ')') && depth > 0 {
                depth -= 1;
            } else if depth == 0
                && matches!(c, '.' | '!' | '?')
                && (i + 1 == chars.len() || chars[i + 1] == ' ')
            {
                let head: String = chars[..=i].iter().collect();
                let last_word = head.rsplit(' ').next().unwrap_or("");
                let initial =
                    last_word.len() == 2 && last_word.starts_with(|x: char| x.is_ascii_uppercase());
                if c != '.' || !(ABBREVIATIONS.contains(&last_word) || initial) {
                    return head;
                }
            }
        }
        i += 1;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolves `Store` and `` `Store::find` `` for the tests.
    fn resolve(t: &str) -> Option<String> {
        match t {
            "Store" | "crate::store::Store" => Some("sym:rs:lending::store#Store".into()),
            "`Store::find`" => Some("sym:rs:lending::store#Store.find".into()),
            "`Service`" => Some(String::new()),
            _ => None,
        }
    }

    /// Normalises a comment with the test resolver.
    fn doc(raw: &str) -> Doc {
        normalise(raw, &resolve, None).unwrap()
    }

    #[test]
    fn intra_doc_links_become_symbol_links() {
        let d = doc("Keeps books for [Store] and [`Store::find`]. See [the trait](crate::store::Store) and [x].");
        assert_eq!(
            d.summary,
            "Keeps books for [Store](sym:rs:lending::store#Store) and [`Store::find`](sym:rs:lending::store#Store.find)."
        );
        assert_eq!(
            d.description[0].text,
            "Keeps books for [Store](sym:rs:lending::store#Store) and [`Store::find`](sym:rs:lending::store#Store.find). See [the trait](sym:rs:lending::store#Store) and \\[x\\]."
        );
    }

    #[test]
    fn links_to_pages_outside_the_map_keep_their_label() {
        let d = doc("A [`Service`] that routes.");
        assert_eq!(d.summary, "A `Service` that routes.");
    }

    #[test]
    fn reference_definitions_resolve_and_drop_out() {
        let d = doc("See [the store][s] and [docs].\n\n[s]: crate::store::Store\n[docs]: https://example.com/a");
        assert_eq!(
            d.description,
            vec![Block {
                kind: "paragraph".into(),
                text: "See [the store](sym:rs:lending::store#Store) and [docs](https://example.com/a).".into(),
                ..Block::default()
            }]
        );
    }

    #[test]
    fn rust_blocks_hide_their_hidden_lines() {
        let d = doc("A book.\n\n# Examples\n\n```\n# use lending::Book;\nlet b = Book::new();\n## not hidden\n```\n\n```text\n# kept\n```");
        assert_eq!(d.description[1].kind, "heading");
        assert_eq!(d.description[2].code, "let b = Book::new();\n# not hidden");
        assert_eq!(d.description[2].language, "rust");
        assert_eq!(d.description[3].code, "# kept");
        assert_eq!(d.description[3].language, "text");
    }

    #[test]
    fn lists_and_sentences() {
        let d = doc("Lends a book, e.g. a novel. Then more.\n\n- one\n- two\n  more");
        assert_eq!(d.summary, "Lends a book, e.g. a novel.");
        assert_eq!(d.description[1].items, vec!["one", "two more"]);
        assert_eq!(first_sentence("Uses `a.b`. Next"), "Uses `a.b`.");
    }

    #[test]
    fn deprecation_alone_makes_a_doc() {
        let d = normalise("", &resolve, Some("use other".into())).unwrap();
        assert_eq!(d.deprecated.as_deref(), Some("use other"));
        assert!(normalise("  ", &resolve, None).is_none());
    }
}
