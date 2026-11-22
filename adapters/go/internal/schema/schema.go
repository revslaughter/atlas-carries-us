// Package schema holds the Go form of the Atlas Carries Us data format,
// version 1. docs/schema.md in the repository root explains every field.
package schema

// Version is the data format version that this package writes.
const Version = 1

// Document is one atlas.json file, or the part of it that an adapter writes.
type Document struct {
	Version     int                  `json:"version"`
	Meta        Meta                 `json:"meta"`
	Containers  []*Container         `json:"containers"`
	Symbols     []*Symbol            `json:"symbols"`
	Edges       []*Edge              `json:"edges"`
	EntryPoints []*EntryPoint        `json:"entryPoints,omitempty"`
	Tests       map[string][]TestUse `json:"tests,omitempty"`
}

// Meta describes the run that produced the document.
type Meta struct {
	SchemaVersion int           `json:"schemaVersion"`
	Tool          Tool          `json:"tool"`
	Generated     string        `json:"generated"`
	Commit        string        `json:"commit,omitempty"`
	Languages     []string      `json:"languages"`
	Adapters      []AdapterInfo `json:"adapters"`
}

// Tool names the program that wrote the document.
type Tool struct {
	Name    string `json:"name"`
	Version string `json:"version"`
}

// AdapterInfo describes one adapter run.
type AdapterInfo struct {
	Name     string   `json:"name"`
	Version  string   `json:"version"`
	Language string   `json:"language"`
	Root     string   `json:"root,omitempty"`
	Notes    []string `json:"notes,omitempty"`
}

// Location points at lines in a file. Path is relative to the repository root.
type Location struct {
	Path    string `json:"path"`
	Line    int    `json:"line"`
	EndLine int    `json:"endLine,omitempty"`
}

// Doc holds a doc comment as written and in normal form.
type Doc struct {
	Raw         string            `json:"raw"`
	Format      string            `json:"format"`
	Summary     string            `json:"summary"`
	Description []Block           `json:"description"`
	Params      map[string]string `json:"params,omitempty"`
	Returns     string            `json:"returns,omitempty"`
	Examples    []Example         `json:"examples,omitempty"`
	SeeAlso     []string          `json:"seeAlso,omitempty"`
	Deprecated  *string           `json:"deprecated,omitempty"`
	Tags        []Tag             `json:"tags,omitempty"`
}

// Example is one code example inside a doc.
type Example struct {
	Title    string `json:"title,omitempty"`
	Code     string `json:"code"`
	Language string `json:"language,omitempty"`
}

// Tag is a block tag that has no field of its own.
type Tag struct {
	Name string `json:"name"`
	Text string `json:"text"`
}

// Block is one block of a doc description: a paragraph, heading, list,
// code block or quote. Only the fields of its type carry values.
type Block struct {
	Type     string   `json:"type"`
	Text     string   `json:"text,omitempty"`
	Level    int      `json:"level,omitempty"`
	Ordered  *bool    `json:"ordered,omitempty"`
	Items    []string `json:"items,omitempty"`
	Code     string   `json:"code,omitempty"`
	Language string   `json:"language,omitempty"`
}

// Container is a module, package, namespace, directory or file.
type Container struct {
	ID              string   `json:"id"`
	Kind            string   `json:"kind"`
	LanguageKind    string   `json:"languageKind"`
	Name            string   `json:"name"`
	Path            string   `json:"path,omitempty"`
	Parent          string   `json:"parent,omitempty"`
	Language        string   `json:"language,omitempty"`
	Unit            bool     `json:"unit,omitempty"`
	Doc             *Doc     `json:"doc,omitempty"`
	ExternalImports []string `json:"externalImports,omitempty"`
}

// Param is one parameter or result of a callable.
type Param struct {
	Name     string   `json:"name,omitempty"`
	Type     string   `json:"type"`
	Refs     []string `json:"refs,omitempty"`
	Optional bool     `json:"optional,omitempty"`
	Rest     bool     `json:"rest,omitempty"`
}

// Signature describes a callable.
type Signature struct {
	TypeParams string  `json:"typeParams,omitempty"`
	Params     []Param `json:"params"`
	Results    []Param `json:"results"`
	Text       string  `json:"text,omitempty"`
}

// TypeText is a type as a reader sees it, with the symbols it names.
type TypeText struct {
	Text string   `json:"text"`
	Refs []string `json:"refs,omitempty"`
}

// Symbol is one declared thing: a type, a callable, a member, a constant or
// a variable.
type Symbol struct {
	ID           string     `json:"id"`
	Kind         string     `json:"kind"`
	LanguageKind string     `json:"languageKind"`
	Name         string     `json:"name"`
	Parent       string     `json:"parent"`
	Language     string     `json:"language,omitempty"`
	Exported     bool       `json:"exported"`
	Location     Location   `json:"location"`
	Doc          *Doc       `json:"doc,omitempty"`
	TypeParams   string     `json:"typeParams,omitempty"`
	Signature    *Signature `json:"signature,omitempty"`
	Type         *TypeText  `json:"type,omitempty"`
	Value        string     `json:"value,omitempty"`
	Group        string     `json:"group,omitempty"`
	Modifiers    []string   `json:"modifiers,omitempty"`
	Annotations  []string   `json:"annotations,omitempty"`
	Satisfies    []string   `json:"satisfies,omitempty"`
}

// Edge is one relation. From uses To.
type Edge struct {
	From       string `json:"from"`
	To         string `json:"to"`
	Kind       string `json:"kind"`
	Confidence string `json:"confidence"`
	Line       int    `json:"line,omitempty"`
	Count      int    `json:"count,omitempty"`
}

// EntryPoint is one way into the code: an HTTP route, a CLI command, and so on.
type EntryPoint struct {
	ID              string   `json:"id"`
	Kind            string   `json:"kind"`
	Name            string   `json:"name"`
	Method          string   `json:"method,omitempty"`
	Path            string   `json:"path,omitempty"`
	Handler         string   `json:"handler"`
	Inline          bool     `json:"inline,omitempty"`
	ExternalHandler string   `json:"externalHandler,omitempty"`
	Registrar       string   `json:"registrar,omitempty"`
	RegisteredAt    Location `json:"registeredAt"`
	Plugin          string   `json:"plugin"`
}

// TestUse is one test that uses a symbol.
type TestUse struct {
	Name     string   `json:"name"`
	Location Location `json:"location"`
}
