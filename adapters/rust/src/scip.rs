//! Reads the SCIP index that `rust-analyzer scip` writes: the documents, and
//! in each one the occurrences of symbols with their ranges.
//!
//! SCIP stores the index as protobuf. This module decodes only the fields
//! that the adapter reads, from the wire format itself, so the adapter needs
//! no protobuf crate. The field numbers come from `scip.proto` in
//! github.com/sourcegraph/scip.

use anyhow::{bail, Result};

/// The role bit that marks a definition; an occurrence without it refers.
pub const DEFINITION: i32 = 1;

/// The part of a SCIP index that the adapter reads.
#[derive(Debug, Default)]
pub struct Index {
    /// One entry per source file.
    pub documents: Vec<Document>,
}

/// The occurrences in one source file.
#[derive(Debug, Default)]
pub struct Document {
    /// The file's path from the project root that rust-analyzer indexed.
    pub relative_path: String,
    /// Every occurrence of a symbol in the file, in the order SCIP writes them.
    pub occurrences: Vec<Occurrence>,
}

/// One place where a symbol appears.
#[derive(Debug, Clone, Default)]
pub struct Occurrence {
    /// Where the name stands.
    pub range: Range,
    /// The symbol, such as "rust-analyzer cargo lending 0.1.0 lib/Book#".
    pub symbol: String,
    /// The role bits; [`DEFINITION`] marks a definition.
    pub roles: i32,
    /// For a definition: the whole item, from its first attribute or doc
    /// comment to its end.
    pub enclosing: Option<Range>,
}

impl Occurrence {
    /// Reports whether the occurrence defines its symbol.
    pub fn is_definition(&self) -> bool {
        self.roles & DEFINITION != 0
    }
}

/// A range of text: lines from 0, columns in UTF-8 bytes from 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range {
    /// The first line.
    pub start_line: u32,
    /// The first column.
    pub start_col: u32,
    /// The last line.
    pub end_line: u32,
    /// The column just after the end.
    pub end_col: u32,
}

impl Range {
    /// Turns SCIP's three or four numbers into a range.
    fn from_numbers(n: &[i64]) -> Option<Range> {
        let u = |i: usize| n.get(i).map(|v| *v as u32);
        match n.len() {
            3 => Some(Range {
                start_line: u(0)?,
                start_col: u(1)?,
                end_line: u(0)?,
                end_col: u(2)?,
            }),
            4 => Some(Range {
                start_line: u(0)?,
                start_col: u(1)?,
                end_line: u(2)?,
                end_col: u(3)?,
            }),
            _ => None,
        }
    }

    /// Reports whether a position lies inside the range.
    pub fn contains(&self, line: u32, col: u32) -> bool {
        (line, col) >= (self.start_line, self.start_col)
            && (line, col) < (self.end_line, self.end_col)
    }
}

/// Reads the fields of one protobuf message, in order.
struct Reader<'a> {
    /// The message's bytes.
    buf: &'a [u8],
    /// The next byte to read.
    pos: usize,
}

/// One field's value, as the wire format carries it.
enum Value<'a> {
    /// A varint.
    Int(u64),
    /// Length-delimited bytes: a string, a message or a packed list.
    Bytes(&'a [u8]),
    /// A fixed-width value that the adapter never reads.
    Fixed,
}

impl<'a> Reader<'a> {
    /// Starts reading a message.
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    /// Reads one varint.
    fn varint(&mut self) -> Result<u64> {
        let mut out = 0u64;
        let mut shift = 0;
        loop {
            let Some(&b) = self.buf.get(self.pos) else {
                bail!("the SCIP index ends inside a number");
            };
            self.pos += 1;
            out |= u64::from(b & 0x7f) << shift;
            if b < 0x80 {
                return Ok(out);
            }
            shift += 7;
            if shift > 63 {
                bail!("the SCIP index holds a number that is too long");
            }
        }
    }

    /// Reads the next field, or returns `None` at the end of the message.
    fn field(&mut self) -> Result<Option<(u32, Value<'a>)>> {
        if self.pos >= self.buf.len() {
            return Ok(None);
        }
        let key = self.varint()?;
        let number = (key >> 3) as u32;
        let value = match key & 7 {
            0 => Value::Int(self.varint()?),
            1 => {
                self.skip(8)?;
                Value::Fixed
            }
            2 => {
                let n = self.varint()? as usize;
                let start = self.pos;
                self.skip(n)?;
                Value::Bytes(&self.buf[start..start + n])
            }
            5 => {
                self.skip(4)?;
                Value::Fixed
            }
            w => {
                bail!("the SCIP index uses protobuf wire type {w}, which this reader does not know")
            }
        };
        Ok(Some((number, value)))
    }

    /// Moves past `n` bytes.
    fn skip(&mut self, n: usize) -> Result<()> {
        if self.pos + n > self.buf.len() {
            bail!("the SCIP index ends inside a field");
        }
        self.pos += n;
        Ok(())
    }
}

/// Reads a packed list of varints.
fn packed(bytes: &[u8]) -> Result<Vec<i64>> {
    let mut r = Reader::new(bytes);
    let mut out = Vec::new();
    while r.pos < bytes.len() {
        out.push(r.varint()? as i64);
    }
    Ok(out)
}

/// Turns bytes into a string, replacing bytes that are not UTF-8.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Decodes a whole SCIP index.
pub fn decode(bytes: &[u8]) -> Result<Index> {
    let mut index = Index::default();
    let mut r = Reader::new(bytes);
    while let Some((n, v)) = r.field()? {
        if let (2, Value::Bytes(b)) = (n, v) {
            index.documents.push(document(b)?);
        }
    }
    Ok(index)
}

/// Decodes one document.
fn document(bytes: &[u8]) -> Result<Document> {
    let mut doc = Document::default();
    let mut r = Reader::new(bytes);
    while let Some((n, v)) = r.field()? {
        match (n, v) {
            (1, Value::Bytes(b)) => doc.relative_path = text(b),
            (2, Value::Bytes(b)) => doc.occurrences.push(occurrence(b)?),
            _ => {}
        }
    }
    Ok(doc)
}

/// Decodes one occurrence.
fn occurrence(bytes: &[u8]) -> Result<Occurrence> {
    let mut occ = Occurrence::default();
    let mut range = Vec::new();
    let mut enclosing = Vec::new();
    let mut r = Reader::new(bytes);
    while let Some((n, v)) = r.field()? {
        match (n, v) {
            (1, Value::Bytes(b)) => range.extend(packed(b)?),
            (1, Value::Int(i)) => range.push(i as i64),
            (2, Value::Bytes(b)) => occ.symbol = text(b),
            (3, Value::Int(i)) => occ.roles = i as i32,
            (7, Value::Bytes(b)) => enclosing.extend(packed(b)?),
            (7, Value::Int(i)) => enclosing.push(i as i64),
            _ => {}
        }
    }
    occ.range = Range::from_numbers(&range).unwrap_or_default();
    occ.enclosing = Range::from_numbers(&enclosing);
    Ok(occ)
}

/// One part of a SCIP symbol's path, such as `store/` or `find().`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    /// The name, without escapes.
    pub name: String,
    /// The suffix that says what the name names: '/' a namespace, '#' a
    /// type, '.' a term, '(' a method, '!' a macro, ':' a meta name, '[' a
    /// type parameter, ')' a parameter.
    pub suffix: char,
}

/// A global SCIP symbol, split into its parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// The package that defines the symbol, such as "lending" or "std".
    pub package: String,
    /// The package's version, or a URL for the standard library.
    pub version: String,
    /// The path inside the package.
    pub descriptors: Vec<Descriptor>,
}

impl Symbol {
    /// The name that the symbol ends with, such as "find" in `Store#find().`.
    pub fn name(&self) -> &str {
        self.descriptors
            .last()
            .map(|d| d.name.as_str())
            .unwrap_or("")
    }

    /// The number of descriptors that name the crate's target: one for `lib/`
    /// and `proc-macro/`, two for `bin/lendd/`, `test/borrowing/` and the like.
    pub fn root_len(&self) -> usize {
        match self.descriptors.first() {
            Some(d)
                if d.suffix == '/'
                    && matches!(d.name.as_str(), "bin" | "test" | "example" | "bench") =>
            {
                2
            }
            Some(d) if d.suffix == '/' => 1,
            _ => 0,
        }
    }

    /// The module path inside the target: the namespaces after the root.
    pub fn module_path(&self) -> Vec<String> {
        self.descriptors
            .iter()
            .skip(self.root_len())
            .take_while(|d| d.suffix == '/')
            .map(|d| d.name.clone())
            .collect()
    }
}

/// Splits a global symbol into its parts. Local symbols (`local 3`) and
/// symbols of another scheme return `None`.
pub fn parse_symbol(s: &str) -> Option<Symbol> {
    let mut parts = s.splitn(5, ' ');
    let scheme = parts.next()?;
    if scheme == "local" {
        return None;
    }
    let _manager = parts.next()?;
    let package = parts.next()?.to_string();
    let version = parts.next()?.to_string();
    let rest: Vec<char> = parts.next()?.chars().collect();
    let mut descriptors = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        match rest[i] {
            '[' => {
                let (name, j) = read_name(&rest, i + 1);
                descriptors.push(Descriptor { name, suffix: '[' });
                i = j + 1;
            }
            '(' => {
                let (name, j) = read_name(&rest, i + 1);
                descriptors.push(Descriptor { name, suffix: ')' });
                i = j + 1;
            }
            _ => {
                let (name, j) = read_name(&rest, i);
                let suffix = *rest.get(j)?;
                i = j + 1;
                if suffix == '(' {
                    while i < rest.len() && rest[i] != ')' {
                        i += 1;
                    }
                    i += 2;
                }
                descriptors.push(Descriptor { name, suffix });
            }
        }
    }
    Some(Symbol {
        package,
        version,
        descriptors,
    })
}

/// Reads one name from position `i`: a name in backticks, or a run of name
/// characters. Returns the name and the position after it.
fn read_name(s: &[char], mut i: usize) -> (String, usize) {
    let mut name = String::new();
    if s.get(i) == Some(&'`') {
        i += 1;
        while i < s.len() {
            if s[i] == '`' {
                if s.get(i + 1) == Some(&'`') {
                    name.push('`');
                    i += 2;
                    continue;
                }
                return (name, i + 1);
            }
            name.push(s[i]);
            i += 1;
        }
        return (name, i);
    }
    while i < s.len() && (s[i].is_alphanumeric() || matches!(s[i], '_' | '+' | '-' | '$')) {
        name.push(s[i]);
        i += 1;
    }
    (name, i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_method_of_a_trait_implementation() {
        let s = parse_symbol(
            "rust-analyzer cargo lending 0.1.0 lib/store/memory/impl#[MemoryStore][Store]find().",
        )
        .unwrap();
        assert_eq!(s.package, "lending");
        assert_eq!(s.module_path(), vec!["store", "memory"]);
        assert_eq!(s.name(), "find");
        assert_eq!(
            s.descriptors[3],
            Descriptor {
                name: "impl".into(),
                suffix: '#'
            }
        );
        assert_eq!(
            s.descriptors[4],
            Descriptor {
                name: "MemoryStore".into(),
                suffix: '['
            }
        );
    }

    #[test]
    fn reads_names_in_backticks_and_binary_roots() {
        let s =
            parse_symbol("rust-analyzer cargo axum 0.0.0 lib/routing/impl#[`Router<S>`]route().")
                .unwrap();
        assert_eq!(s.descriptors[3].name, "Router<S>");
        let b = parse_symbol("rust-analyzer cargo lendd 0.1.0 bin/lendd/http/router().").unwrap();
        assert_eq!(b.root_len(), 2);
        assert_eq!(b.module_path(), vec!["http"]);
        assert!(parse_symbol("local 4").is_none());
    }

    #[test]
    fn decodes_an_occurrence() {
        // Field 1 (packed range 2, 4, 9), field 2 (symbol "s"), field 3 (role 1).
        let occ = [0x0a, 3, 2, 4, 9, 0x12, 1, b's', 0x18, 1];
        let mut doc = vec![0x0a, 4, b'a', b'.', b'r', b's', 0x12, occ.len() as u8];
        doc.extend_from_slice(&occ);
        let mut index = vec![0x12, doc.len() as u8];
        index.extend_from_slice(&doc);
        let got = decode(&index).unwrap();
        let o = &got.documents[0].occurrences[0];
        assert_eq!(got.documents[0].relative_path, "a.rs");
        assert_eq!(
            o.range,
            Range {
                start_line: 2,
                start_col: 4,
                end_line: 2,
                end_col: 9
            }
        );
        assert!(o.is_definition());
    }
}
