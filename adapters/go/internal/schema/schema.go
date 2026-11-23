// Package schema holds the Go form of the Atlas Carries Us data format,
// version 1: the types that the Go adapter fills and writes as JSON.
//
// The JSON Schema in schema/atlas.schema.json and the spec in docs/schema.md
// define the same format; the TypeScript adapter (adapters/typescript/src/schema.ts)
// and the viewer (viewer/src/types.ts) hold their own copies. A change to the
// format touches all five places in one pull request.
package schema

// Version is the data format version that this package writes.
const Version = 1

// Document is one atlas.json file, or the part of it that an adapter writes.
// The acu command adds the project, regions, layout and tour.
type Document struct {
	// Version holds the data format version: always [Version].
	Version int `json:"version"`
	// Meta describes the run that wrote the document.
	Meta Meta `json:"meta"`
	// Containers lists the modules, packages, folders and files, sorted by ID.
	Containers []*Container `json:"containers"`
	// Symbols lists every declared thing, sorted by ID.
	Symbols []*Symbol `json:"symbols"`
	// Edges lists the relations, each from the user to the thing used.
	Edges []*Edge `json:"edges"`
	// EntryPoints lists the ways into the code that the plugins found.
	EntryPoints []*EntryPoint `json:"entryPoints,omitempty"`
	// Tests maps a symbol ID to the tests that use the symbol.
	Tests map[string][]TestUse `json:"tests,omitempty"`
}

// Meta describes the run that produced the document.
type Meta struct {
	// SchemaVersion repeats [Version], for readers that look here first.
	SchemaVersion int `json:"schemaVersion"`
	// Tool names the program that wrote the final file.
	Tool Tool `json:"tool"`
	// Generated holds the time of the run, RFC 3339, in UTC.
	Generated string `json:"generated"`
	// Commit names the commit that the adapters read, when known.
	Commit string `json:"commit,omitempty"`
	// Languages lists the language tags in the document, such as "go".
	Languages []string `json:"languages"`
	// Adapters lists one entry per adapter run.
	Adapters []AdapterInfo `json:"adapters"`
}

// Tool names the program that wrote the document.
type Tool struct {
	// Name holds the program's name, such as "acu-adapter-go".
	Name string `json:"name"`
	// Version holds the program's release.
	Version string `json:"version"`
}

// AdapterInfo describes one adapter run.
type AdapterInfo struct {
	// Name holds the adapter's name as atlas.yaml writes it, such as "go".
	Name string `json:"name"`
	// Version holds the adapter's release.
	Version string `json:"version"`
	// Language holds the language tag that starts the run's IDs.
	Language string `json:"language"`
	// Root holds the folder that the adapter read, relative to the repository root.
	Root string `json:"root,omitempty"`
	// Notes lists the limits of the run in plain words; the viewer's help shows them.
	Notes []string `json:"notes,omitempty"`
}

// Location points at lines in a file. Path is relative to the repository root.
type Location struct {
	// Path holds the file's path from the repository root, with forward slashes.
	Path string `json:"path"`
	// Line holds the first line, counting from 1.
	Line int `json:"line"`
	// EndLine holds the last line, when the location spans more than one.
	EndLine int `json:"endLine,omitempty"`
}

// Doc holds a doc comment as written and in normal form. The viewer reads
// only the normal form, so it never needs to know a language's comment style.
type Doc struct {
	// Raw holds the comment as written, without comment markers.
	Raw string `json:"raw"`
	// Format names the comment style; the Go adapter writes "godoc".
	Format string `json:"format"`
	// Summary holds the first sentence, in the inline subset. The map draws it on every box.
	Summary string `json:"summary"`
	// Description holds the whole comment as a list of blocks.
	Description []Block `json:"description"`
	// Params maps a parameter's name to its text.
	Params map[string]string `json:"params,omitempty"`
	// Returns holds the text about the result.
	Returns string `json:"returns,omitempty"`
	// Examples lists code examples.
	Examples []Example `json:"examples,omitempty"`
	// SeeAlso lists related things, in the inline subset.
	SeeAlso []string `json:"seeAlso,omitempty"`
	// Deprecated holds the deprecation note; an empty string still marks the item deprecated.
	Deprecated *string `json:"deprecated,omitempty"`
	// Tags lists the block tags that have no field of their own.
	Tags []Tag `json:"tags,omitempty"`
}

// Example is one code example inside a doc.
type Example struct {
	// Title names the example, when it has a name.
	Title string `json:"title,omitempty"`
	// Code holds the example's code, as written.
	Code string `json:"code"`
	// Language names the code's language, such as "go".
	Language string `json:"language,omitempty"`
}

// Tag is a block tag that has no field of its own.
type Tag struct {
	// Name holds the tag's name, without its marker.
	Name string `json:"name"`
	// Text holds the tag's text, in the inline subset.
	Text string `json:"text"`
}

// Block is one block of a doc description: a paragraph, heading, list,
// code block or quote. Only the fields of its type carry values.
type Block struct {
	// Type names the block: "paragraph", "heading", "list", "code" or "quote".
	Type string `json:"type"`
	// Text holds the text of a paragraph, heading or quote, in the inline subset.
	Text string `json:"text,omitempty"`
	// Level holds a heading's level, from 1 to 6.
	Level int `json:"level,omitempty"`
	// Ordered reports whether a list counts its items.
	Ordered *bool `json:"ordered,omitempty"`
	// Items lists a list's items, in the inline subset.
	Items []string `json:"items,omitempty"`
	// Code holds a code block's code, as written.
	Code string `json:"code,omitempty"`
	// Language names a code block's language, when known.
	Language string `json:"language,omitempty"`
}

// Container is a module, package, namespace, directory or file. Containers
// form a tree through Parent; a Go tree runs module, package, file.
type Container struct {
	// ID names the container, such as "go:pkg:example.com/lending/internal/store".
	ID string `json:"id"`
	// Kind holds the neutral kind: "module", "package", "namespace", "directory" or "file".
	Kind string `json:"kind"`
	// LanguageKind holds the language's own word, which the viewer shows.
	LanguageKind string `json:"languageKind"`
	// Name holds a short display name, such as "internal/store".
	Name string `json:"name"`
	// Path holds the container's path from the repository root.
	Path string `json:"path,omitempty"`
	// Parent names the enclosing container.
	Parent string `json:"parent,omitempty"`
	// Language holds the language tag.
	Language string `json:"language,omitempty"`
	// Unit marks a container that the overview draws as a box: a Go package.
	Unit bool `json:"unit,omitempty"`
	// Doc holds the container's doc: the package doc, or a file's comment.
	Doc *Doc `json:"doc,omitempty"`
	// ExternalImports lists the outside modules that the container imports.
	ExternalImports []string `json:"externalImports,omitempty"`
}

// Param is one parameter or result of a callable.
type Param struct {
	// Name holds the parameter's name; a result may have none.
	Name string `json:"name,omitempty"`
	// Type holds the type as a reader sees it.
	Type string `json:"type"`
	// Refs lists the IDs of the symbols that Type names, so the viewer can link them.
	Refs []string `json:"refs,omitempty"`
	// Optional marks a parameter that a caller may leave out.
	Optional bool `json:"optional,omitempty"`
	// Rest marks a parameter that takes any number of values.
	Rest bool `json:"rest,omitempty"`
}

// Signature describes a callable: its parameters, results and type parameters.
type Signature struct {
	// TypeParams holds the type parameters as written, such as "[T any]".
	TypeParams string `json:"typeParams,omitempty"`
	// Params lists the parameters in order.
	Params []Param `json:"params"`
	// Results lists the results in order.
	Results []Param `json:"results"`
	// Text holds the whole signature as the language writes it.
	Text string `json:"text,omitempty"`
}

// TypeText is a type as a reader sees it, with the symbols it names.
type TypeText struct {
	// Text holds the type as written.
	Text string `json:"text"`
	// Refs lists the IDs of the symbols that Text names.
	Refs []string `json:"refs,omitempty"`
}

// Symbol is one declared thing: a type, a callable, a member, a constant or
// a variable. A member names its owner in Parent; a top-level symbol names
// its file.
type Symbol struct {
	// ID names the symbol: the unit's qualified name, "#", and the symbol's path.
	ID string `json:"id"`
	// Kind holds the neutral kind, such as "struct", "method" or "field".
	Kind string `json:"kind"`
	// LanguageKind holds the language's own word, which the viewer shows.
	LanguageKind string `json:"languageKind"`
	// Name holds the declared name.
	Name string `json:"name"`
	// Parent names the owning symbol, or the file of a top-level symbol.
	Parent string `json:"parent"`
	// Language holds the language tag.
	Language string `json:"language,omitempty"`
	// Exported reports whether code outside the unit may use the symbol.
	Exported bool `json:"exported"`
	// Location points at the declaration.
	Location Location `json:"location"`
	// Doc holds the doc comment.
	Doc *Doc `json:"doc,omitempty"`
	// TypeParams holds a generic type's type parameters, as written.
	TypeParams string `json:"typeParams,omitempty"`
	// Signature describes a function or method.
	Signature *Signature `json:"signature,omitempty"`
	// Type holds the type of a field, constant or variable, or the type under a named type.
	Type *TypeText `json:"type,omitempty"`
	// Value holds a constant's value, cut to 80 characters.
	Value string `json:"value,omitempty"`
	// Group joins the declarations of one const or var block; the key stays unique in a file.
	Group string `json:"group,omitempty"`
	// Modifiers lists words such as "embedded" and "pointer receiver".
	Modifiers []string `json:"modifiers,omitempty"`
	// Annotations lists a field's struct tags, as written.
	Annotations []string `json:"annotations,omitempty"`
	// Satisfies lists well-known outside interfaces that the type satisfies, such as "error".
	Satisfies []string `json:"satisfies,omitempty"`
}

// Edge is one relation. From uses To: a caller its callee, a type the
// interface it implements, a function its parameter's type.
type Edge struct {
	// From names the user.
	From string `json:"from"`
	// To names the thing used.
	To string `json:"to"`
	// Kind names the relation, such as "calls" or "implements".
	Kind string `json:"kind"`
	// Confidence holds "exact" when a type checker resolved the edge; the Go adapter always writes it.
	Confidence string `json:"confidence"`
	// Line holds the first line of the use, in the file of From.
	Line int `json:"line,omitempty"`
	// Count holds how many places make the same use, when more than one.
	Count int `json:"count,omitempty"`
}

// EntryPoint is one way into the code: an HTTP route, a CLI command, and so on.
type EntryPoint struct {
	// ID names the entry point by kind and name, such as "http:GET /api/books/{id}".
	ID string `json:"id"`
	// Kind holds "http", "cli", "message", "job" or "grpc".
	Kind string `json:"kind"`
	// Name holds the route or command as a reader writes it.
	Name string `json:"name"`
	// Method holds an HTTP route's method.
	Method string `json:"method,omitempty"`
	// Path holds an HTTP route's path.
	Path string `json:"path,omitempty"`
	// Handler names the function or method that serves the entry point.
	Handler string `json:"handler"`
	// Inline marks a handler written inline; Handler then names the registering function.
	Inline bool `json:"inline,omitempty"`
	// ExternalHandler names a handler from outside the code, such as "pprof.Index".
	ExternalHandler string `json:"externalHandler,omitempty"`
	// Registrar names the function that registers the entry point.
	Registrar string `json:"registrar,omitempty"`
	// RegisteredAt points at the registration.
	RegisteredAt Location `json:"registeredAt"`
	// Plugin names the plugin that found the entry point, such as "go-chi".
	Plugin string `json:"plugin"`
}

// TestUse is one test that uses a symbol.
type TestUse struct {
	// Name holds the test function's name, such as "TestBorrow".
	Name string `json:"name"`
	// Location points at the test function.
	Location Location `json:"location"`
}
