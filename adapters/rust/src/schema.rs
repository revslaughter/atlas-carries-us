//! The Atlas Carries Us data format, version 1, as the Rust types that the
//! adapter fills and writes as JSON.
//!
//! The JSON Schema in `schema/atlas.schema.json` and the spec in
//! `docs/schema.md` define the same format. The other adapters and the viewer
//! hold their own copies; a change to the format touches every copy in one
//! pull request.

use std::collections::BTreeMap;

use serde::Serialize;

/// The data format version that this module writes.
pub const VERSION: u32 = 1;

/// One atlas.json file, or the part of it that an adapter writes. The acu
/// command adds the project, regions, layout and tour.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    /// The data format version: always [`VERSION`].
    pub version: u32,
    /// Describes the run that wrote the document.
    pub meta: Meta,
    /// The crates, modules and files, sorted by ID.
    pub containers: Vec<Container>,
    /// Every declared thing, sorted by ID.
    pub symbols: Vec<Symbol>,
    /// The relations, each from the user to the thing used.
    pub edges: Vec<Edge>,
    /// The ways into the code that the plugins found.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entry_points: Vec<EntryPoint>,
    /// Maps a symbol ID to the tests that use the symbol.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tests: BTreeMap<String, Vec<TestUse>>,
}

/// Describes the run that produced the document.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    /// Repeats [`VERSION`], for readers that look here first.
    pub schema_version: u32,
    /// Names the program that wrote the final file.
    pub tool: Tool,
    /// The time of the run, RFC 3339, in UTC.
    pub generated: String,
    /// The commit that the adapter read, when known.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub commit: String,
    /// The language tags in the document: `["rust"]`.
    pub languages: Vec<String>,
    /// One entry per adapter run.
    pub adapters: Vec<AdapterInfo>,
}

/// Names the program that wrote the document.
#[derive(Debug, Serialize)]
pub struct Tool {
    /// The program's name, such as "acu-adapter-rust".
    pub name: String,
    /// The program's release.
    pub version: String,
}

/// Describes one adapter run.
#[derive(Debug, Serialize)]
pub struct AdapterInfo {
    /// The adapter's name as atlas.yaml writes it: "rust".
    pub name: String,
    /// The adapter's release.
    pub version: String,
    /// The language tag that starts the run's IDs.
    pub language: String,
    /// The folder that the adapter read, relative to the repository root.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub root: String,
    /// The limits of the run in plain words; the viewer's help shows them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// Points at lines in a file. The path starts at the repository root.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    /// The file's path from the repository root, with forward slashes.
    pub path: String,
    /// The first line, counting from 1.
    pub line: u32,
    /// The last line, when the location spans more than one.
    #[serde(skip_serializing_if = "is_zero")]
    pub end_line: u32,
}

/// Reports whether a number holds zero, so that serde leaves it out.
fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Reports whether a flag holds false, so that serde leaves it out.
fn is_false(b: &bool) -> bool {
    !*b
}

/// A doc comment as written and in normal form. The viewer reads only the
/// normal form, so it never needs to know a language's comment style.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Doc {
    /// The comment as written, without comment markers.
    pub raw: String,
    /// The comment style: "rustdoc".
    pub format: String,
    /// The first sentence, in the inline subset. The map draws it on every box.
    pub summary: String,
    /// The whole comment as a list of blocks.
    pub description: Vec<Block>,
    /// The text of a deprecation; an empty string still marks the item deprecated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<String>,
}

/// One block of a doc description: a paragraph, heading, list, code block or
/// quote. Only the fields of its type carry values.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Block {
    /// Names the block: "paragraph", "heading", "list", "code" or "quote".
    #[serde(rename = "type")]
    pub kind: String,
    /// The text of a paragraph, heading or quote, in the inline subset.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// A heading's level, from 1 to 6.
    #[serde(skip_serializing_if = "is_zero")]
    pub level: u32,
    /// Whether a list counts its items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordered: Option<bool>,
    /// A list's items, in the inline subset.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    /// A code block's code, as written.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub code: String,
    /// A code block's language, when known.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub language: String,
}

/// A crate, module or file. Containers form a tree through `parent`; a Rust
/// tree runs crate, module, file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    /// Names the container, such as "rs:mod:lending::store".
    pub id: String,
    /// The neutral kind: "package", "module" or "file".
    pub kind: String,
    /// The language's own word, which the viewer shows.
    pub language_kind: String,
    /// A short display name, such as "lending::store".
    pub name: String,
    /// The container's path from the repository root.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// The enclosing container.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub parent: String,
    /// The language tag: "rust".
    pub language: String,
    /// Marks a container that the overview draws as a box: a module.
    #[serde(skip_serializing_if = "is_false")]
    pub unit: bool,
    /// The container's doc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<Doc>,
    /// The outside packages that the container's code names.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub external_imports: Vec<String>,
}

/// One parameter or result of a callable.
#[derive(Debug, Clone, Serialize)]
pub struct Param {
    /// The parameter's name; a result has none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The type as a reader sees it.
    #[serde(rename = "type")]
    pub type_text: String,
    /// The IDs of the symbols that the type names, so the viewer can link them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
}

/// Describes a callable: its parameters, results and type parameters.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    /// The type parameters as written, such as "<S: Store>".
    #[serde(skip_serializing_if = "String::is_empty")]
    pub type_params: String,
    /// The parameters in order.
    pub params: Vec<Param>,
    /// The results in order: one, or none.
    pub results: Vec<Param>,
    /// The whole signature as Rust writes it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// A type as a reader sees it, with the symbols it names.
#[derive(Debug, Clone, Serialize)]
pub struct TypeText {
    /// The type as written.
    pub text: String,
    /// The IDs of the symbols that the text names.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
}

/// One declared thing: a type, a callable, a member, a constant or a static.
/// A member names its owner in `parent`; a top-level symbol names its file or
/// its inline module.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    /// Names the symbol: the module path, "#", and the symbol's path.
    pub id: String,
    /// The neutral kind, such as "struct", "method" or "field".
    pub kind: String,
    /// The language's own word, which the viewer shows.
    pub language_kind: String,
    /// The declared name.
    pub name: String,
    /// The owning symbol, or the container of a top-level symbol.
    pub parent: String,
    /// The language tag: "rust".
    pub language: String,
    /// Whether code outside the module may use the symbol.
    pub exported: bool,
    /// Points at the declaration.
    pub location: Location,
    /// The doc comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<Doc>,
    /// A generic type's parameters, as written.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub type_params: String,
    /// Describes a function or method.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
    /// The type of a field, constant or static, or the type under an alias.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_text: Option<TypeText>,
    /// A constant's value, cut to 80 characters.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    /// Words such as "async", "unsafe" and "mut".
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<String>,
    /// Attributes as written, such as "#[derive(Clone, Debug)]".
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<String>,
    /// The outside traits that the type implements, such as "Display".
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub satisfies: Vec<String>,
}

/// One relation. `from` uses `to`: a caller its callee, a type the trait it
/// implements, a function its parameter's type.
#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    /// The user.
    pub from: String,
    /// The thing used.
    pub to: String,
    /// The relation, such as "calls" or "implements".
    pub kind: String,
    /// "exact": rustdoc or rust-analyzer resolved the edge.
    pub confidence: String,
    /// The first line of the use, in the file of `from`.
    #[serde(skip_serializing_if = "is_zero")]
    pub line: u32,
    /// How many places make the same use, when more than one.
    #[serde(skip_serializing_if = "is_zero")]
    pub count: u32,
}

/// One way into the code: an HTTP route or a CLI command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryPoint {
    /// Names the entry point by kind and name, such as "http:GET /books".
    pub id: String,
    /// "http" or "cli".
    pub kind: String,
    /// The route or command as a reader writes it.
    pub name: String,
    /// An HTTP route's method.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub method: String,
    /// An HTTP route's path.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// The function or method that serves the entry point.
    pub handler: String,
    /// Marks a handler written inline; `handler` then names the registering function.
    #[serde(skip_serializing_if = "is_false")]
    pub inline: bool,
    /// A handler from outside the code, as written.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub external_handler: String,
    /// The function that registers the entry point.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub registrar: String,
    /// Points at the registration.
    pub registered_at: Location,
    /// The plugin that found the entry point: "axum" or "clap".
    pub plugin: String,
}

/// One test that uses a symbol.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct TestUse {
    /// The test's name, as `cargo test` prints it.
    pub name: String,
    /// Points at the test function.
    pub location: Location,
}
