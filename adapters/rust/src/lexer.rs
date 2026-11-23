//! A small lexer for Rust source. The plugins read tokens around a
//! registration (`.route("/x", get(h))`, `#[derive(Parser)]`), and SCIP
//! resolves the names among them. The lexer knows comments, strings, raw
//! strings, characters and lifetimes, so that none of them hides or fakes a
//! token; it knows nothing of Rust's grammar.

/// What a token holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A name or keyword, such as `route` or `fn`.
    Ident,
    /// A string literal; `text` holds its value.
    Str,
    /// A number, a character or a lifetime.
    Other,
    /// Punctuation: one character, or `::`, `=>` or `->`.
    Punct,
}

/// One token, with the place where it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// What the token holds.
    pub kind: Kind,
    /// The token as written; for a string, its value.
    pub text: String,
    /// The line, from 0.
    pub line: u32,
    /// The column, in UTF-8 bytes from 0, as SCIP counts.
    pub col: u32,
}

impl Token {
    /// Reports whether the token is the punctuation `p`.
    pub fn is(&self, p: &str) -> bool {
        self.kind == Kind::Punct && self.text == p
    }

    /// Reports whether the token is the name or keyword `w`.
    pub fn is_ident(&self, w: &str) -> bool {
        self.kind == Kind::Ident && self.text == w
    }
}

/// Walks the characters of a source text and keeps the line and column.
struct Scanner<'a> {
    /// The characters with their byte offsets.
    chars: Vec<(usize, char)>,
    /// The source.
    src: &'a str,
    /// The next character.
    i: usize,
    /// The current line.
    line: u32,
    /// The byte offset where the current line starts.
    line_start: usize,
}

impl Scanner<'_> {
    /// Returns the character `k` places ahead, or a NUL past the end.
    fn peek(&self, k: usize) -> char {
        self.chars.get(self.i + k).map(|c| c.1).unwrap_or('\0')
    }

    /// Moves one character on, and counts lines.
    fn bump(&mut self) -> char {
        let (off, c) = self.chars[self.i];
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.line_start = off + 1;
        }
        c
    }

    /// The column of the next character.
    fn col(&self) -> u32 {
        let off = self
            .chars
            .get(self.i)
            .map(|c| c.0)
            .unwrap_or(self.src.len());
        (off - self.line_start) as u32
    }

    /// Reports whether the scan has reached the end.
    fn done(&self) -> bool {
        self.i >= self.chars.len()
    }
}

/// Splits Rust source into tokens. Comments and whitespace drop out.
pub fn lex(src: &str) -> Vec<Token> {
    let mut s = Scanner {
        chars: src.char_indices().collect(),
        src,
        i: 0,
        line: 0,
        line_start: 0,
    };
    let mut out = Vec::new();
    while !s.done() {
        let c = s.peek(0);
        if c.is_whitespace() {
            s.bump();
            continue;
        }
        if c == '/' && s.peek(1) == '/' {
            while !s.done() && s.peek(0) != '\n' {
                s.bump();
            }
            continue;
        }
        if c == '/' && s.peek(1) == '*' {
            block_comment(&mut s);
            continue;
        }
        let (line, col) = (s.line, s.col());
        let push = |out: &mut Vec<Token>, kind, text: String| {
            out.push(Token {
                kind,
                text,
                line,
                col,
            })
        };
        if let Some(value) = string(&mut s) {
            push(&mut out, Kind::Str, value);
            continue;
        }
        if c == '\'' {
            push(&mut out, Kind::Other, quote(&mut s));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut name = String::new();
            if c == 'r' && s.peek(1) == '#' && (s.peek(2).is_alphabetic() || s.peek(2) == '_') {
                s.bump();
                s.bump();
            }
            while s.peek(0).is_alphanumeric() || s.peek(0) == '_' {
                name.push(s.bump());
            }
            push(&mut out, Kind::Ident, name);
            continue;
        }
        if c.is_ascii_digit() {
            let mut num = String::new();
            while s.peek(0).is_alphanumeric()
                || s.peek(0) == '_'
                || (s.peek(0) == '.' && s.peek(1).is_ascii_digit())
            {
                num.push(s.bump());
            }
            push(&mut out, Kind::Other, num);
            continue;
        }
        let two: String = [c, s.peek(1)].iter().collect();
        if matches!(two.as_str(), "::" | "=>" | "->") {
            s.bump();
            s.bump();
            push(&mut out, Kind::Punct, two);
            continue;
        }
        s.bump();
        push(&mut out, Kind::Punct, c.to_string());
    }
    out
}

/// Skips a block comment, which may nest.
fn block_comment(s: &mut Scanner) {
    let mut depth = 0;
    while !s.done() {
        if s.peek(0) == '/' && s.peek(1) == '*' {
            depth += 1;
            s.bump();
            s.bump();
        } else if s.peek(0) == '*' && s.peek(1) == '/' {
            depth -= 1;
            s.bump();
            s.bump();
            if depth == 0 {
                return;
            }
        } else {
            s.bump();
        }
    }
}

/// Reads a string literal at the scanner's place, if one starts there:
/// `"…"`, `b"…"`, `c"…"`, and the raw forms `r"…"` and `r#"…"#`. Returns its
/// value, with the common escapes undone.
fn string(s: &mut Scanner) -> Option<String> {
    let mut k = 0;
    if matches!(s.peek(0), 'b' | 'c') {
        k = 1;
    }
    let raw = s.peek(k) == 'r';
    if raw {
        k += 1;
    }
    let mut hashes = 0;
    if raw {
        while s.peek(k + hashes) == '#' {
            hashes += 1;
        }
    }
    if s.peek(k + hashes) != '"' {
        return None;
    }
    for _ in 0..=k + hashes {
        s.bump();
    }
    let mut value = String::new();
    while !s.done() {
        let c = s.bump();
        if c == '"' && (0..hashes).all(|h| s.peek(h) == '#') {
            for _ in 0..hashes {
                s.bump();
            }
            return Some(value);
        }
        if c == '\\' && !raw {
            let e = s.bump();
            match e {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                '\n' => {
                    while s.peek(0).is_whitespace() {
                        s.bump();
                    }
                }
                other => value.push(other),
            }
            continue;
        }
        value.push(c);
    }
    Some(value)
}

/// Reads a character literal or a lifetime, both of which start with `'`.
fn quote(s: &mut Scanner) -> String {
    let mut text = String::from(s.bump());
    if s.peek(0) == '\\' {
        while !s.done() && s.peek(0) != '\'' {
            if s.peek(0) == '\\' {
                text.push(s.bump());
            }
            text.push(s.bump());
        }
        if !s.done() {
            text.push(s.bump());
        }
        return text;
    }
    if s.peek(1) == '\'' {
        text.push(s.bump());
        text.push(s.bump());
        return text;
    }
    while s.peek(0).is_alphanumeric() || s.peek(0) == '_' {
        text.push(s.bump());
    }
    text
}

/// Finds the token that closes the bracket at `open`: `)`, `]` or `}`.
/// Returns the index of the closing token, or the last index.
pub fn closing(tokens: &[Token], open: usize) -> usize {
    let mut depth = 0i32;
    for (i, t) in tokens.iter().enumerate().skip(open) {
        if t.kind != Kind::Punct {
            continue;
        }
        match t.text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    tokens.len().saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lexes a text and returns the tokens' texts.
    fn texts(src: &str) -> Vec<String> {
        lex(src).into_iter().map(|t| t.text).collect()
    }

    #[test]
    fn strings_and_comments_hide_nothing() {
        assert_eq!(
            texts(r##"a.route("/x", get(h)) // .route("/y")"##),
            ["a", ".", "route", "(", "/x", ",", "get", "(", "h", ")", ")"]
        );
        assert_eq!(texts(r##"r#"a"b"# /* x /* y */ z */ c"##), ["a\"b", "c"]);
    }

    #[test]
    fn lifetimes_and_characters_differ() {
        assert_eq!(texts("fn f<'a>(x: &'a str) -> char { 'x' }")[3], "'a");
        assert_eq!(texts("'x'"), ["'x'"]);
        assert_eq!(texts(r"'\n'"), [r"'\n'"]);
    }

    #[test]
    fn positions_count_lines_and_bytes() {
        let t = lex("é a\n  b::c");
        assert_eq!((t[0].line, t[0].col), (0, 0));
        assert_eq!((t[1].line, t[1].col), (0, 3));
        assert_eq!((t[2].line, t[2].col), (1, 2));
        assert!(t[3].is("::"));
    }

    #[test]
    fn closing_finds_the_matching_bracket() {
        let t = lex("f(a, (b), [c]) d");
        assert_eq!(t[closing(&t, 1)].text, ")");
        assert_eq!(closing(&t, 1), 11);
    }
}
