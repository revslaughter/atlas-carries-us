// The data format, version 1, as the viewer reads it: what the adapters
// write, plus the project, regions, layout and tour that acu adds from
// atlas.yaml. docs/schema.md explains every field; the adapters hold their
// own copies (adapters/go/internal/schema, adapters/typescript/src/schema.ts).

/** Points at lines in a file. The path is relative to the repository root. */
export interface Location {
  /** The file's path from the repository root, with forward slashes. */
  path: string;
  /** The first line, counting from 1. */
  line: number;
  /** The last line, when the location spans more than one. */
  endLine?: number;
}

/** Marks text that a program drafted. The viewer labels it until a person checks it and removes the mark. */
export interface Provenance {
  /** The program or model that drafted the text. */
  by: string;
  /** When it drafted the text, RFC 3339. */
  at: string;
}

/** One block of a doc description: a paragraph, heading, list, code block or quote. */
export type Block =
  | { type: "paragraph"; text: string }
  | { type: "heading"; level: number; text: string }
  | { type: "list"; ordered: boolean; items: string[] }
  | { type: "code"; code: string; language?: string }
  | { type: "quote"; text: string };

/** A doc comment in normal form: the viewer draws it without knowing the language's comment style. */
export interface Doc {
  /** The comment as written, without comment markers; "Show the comment as written" reveals it. */
  raw: string;
  /** The comment style, such as "godoc" or "tsdoc". */
  format: string;
  /** The first sentence, in the inline subset. Every box shows it under the name. */
  summary: string;
  /** The whole comment as blocks. */
  description: Block[];
  /** Each parameter's text, by name. */
  params?: Record<string, string>;
  /** Each type parameter's text, by name. */
  typeParams?: Record<string, string>;
  /** The text about the result. */
  returns?: string;
  /** What the code throws, and when. */
  throws?: { type?: string; text: string }[];
  /** Code examples. */
  examples?: { title?: string; code: string; language?: string }[];
  /** Related things, in the inline subset. */
  seeAlso?: string[];
  /** The deprecation note; an empty string still marks the item deprecated. */
  deprecated?: string;
  /** The version that added the item. */
  since?: string;
  /** Other block tags, shown as notes. */
  tags?: { name: string; text: string }[];
}

/** One parameter or result of a callable. */
export interface Param {
  /** The parameter's name; a result may have none. */
  name?: string;
  /** The type as a reader sees it. */
  type: string;
  /** The IDs of the symbols that the type names, which the panel turns into links. */
  refs?: string[];
  /** Marks a parameter that a caller may leave out. */
  optional?: boolean;
  /** Marks a parameter that takes any number of values. */
  rest?: boolean;
  /** The default value, as written. */
  default?: string;
}

/** Describes a callable: its parameters, results and type parameters. */
export interface Signature {
  /** The type parameters as written. */
  typeParams?: string;
  /** The parameters in order. */
  params: Param[];
  /** The results in order. */
  results: Param[];
  /** The whole signature as the language writes it, shown in the panel. */
  text?: string;
}

/** A type as a reader sees it, with the symbols it names. */
export interface TypeText {
  /** The type as written. */
  text: string;
  /** The IDs of the symbols that the type names. */
  refs?: string[];
}

/** A module, package, namespace, folder or file. Containers form a tree through `parent`. */
export interface Container {
  /** The container's ID. */
  id: string;
  /** The neutral kind. The viewer treats files apart: a unit lists its files. */
  kind: "module" | "package" | "namespace" | "directory" | "file";
  /** The language's own word, which the viewer shows for units: the unit word. */
  languageKind: string;
  /** A short display name. */
  name: string;
  /** The container's path from the repository root. */
  path?: string;
  /** The enclosing container. */
  parent?: string;
  /** The language tag. */
  language?: string;
  /** Marks a unit: a container that the overview draws as a box and that opens a unit focus. */
  unit?: boolean;
  /** The container's doc. */
  doc?: Doc;
  /** The outside libraries that the container imports. */
  externalImports?: string[];
}

/** One declared thing: a type, a callable, a member, a constant or a variable. */
export interface Symbol {
  /** The symbol's ID, which links use: `#symbol=<id>`. */
  id: string;
  /** The neutral kind, such as "class", "method" or "constant". */
  kind: string;
  /** The language's own word, such as "struct" or "arrow function", shown on boxes. */
  languageKind: string;
  /** The declared name. */
  name: string;
  /** The owning symbol (for a member), or the file (for a top-level symbol). */
  parent: string;
  /** The language tag. */
  language?: string;
  /** Reports whether code outside the unit may use the symbol; the panel marks the rest "not exported". */
  exported: boolean;
  /** Where the declaration sits; the panel links to it. */
  location: Location;
  /** The doc comment. */
  doc?: Doc;
  /** A generic type's type parameters, shown after its name. */
  typeParams?: string;
  /** The signature of a callable. */
  signature?: Signature;
  /** The type of a field, property, constant or variable, or the type under a named type. */
  type?: TypeText;
  /** A constant's value, shortened. */
  value?: string;
  /** Joins declarations written together, such as one block of constants. */
  group?: string;
  /** Words such as "static", "async" or "embedded". */
  modifiers?: string[];
  /** Decorators, attributes or field tags, as written. */
  annotations?: string[];
  /** Well-known outside interfaces that the symbol satisfies. */
  satisfies?: string[];
}

/** The relations that an edge can carry. Each one points from the user to the thing used. */
export type EdgeKind =
  "calls" | "implements" | "extends" | "field_type" | "param_type" | "return_type" | "imports" | "overrides";

/** One relation: `from` uses `to`. */
export interface Edge {
  /** The user. */
  from: string;
  /** The thing used. */
  to: string;
  /** The relation. */
  kind: EdgeKind;
  /** "exact" when a type checker resolved the edge; the viewer draws "heuristic" edges fainter and says so. */
  confidence: "exact" | "heuristic";
  /** The first line of the use, in the file of `from`; a trace numbers calls in this order. */
  line?: number;
  /** How many places make the same use. */
  count?: number;
}

/** One way into the code: an HTTP route, a CLI command, a message handler and so on. */
export interface EntryPoint {
  /** The entry point's ID, such as "http:GET /api/books/{id}". */
  id: string;
  /** The kind of entry point. */
  kind: "http" | "cli" | "message" | "job" | "grpc";
  /** The route or command as a reader writes it. */
  name: string;
  /** An HTTP route's method. */
  method?: string;
  /** An HTTP route's path. */
  path?: string;
  /** The function or method that serves it: where a trace starts. */
  handler: string;
  /** Marks a handler written inline; `handler` then names the registering function. */
  inline?: boolean;
  /** The handler, when it lives outside the code. */
  externalHandler?: string;
  /** The function that registers the entry point. */
  registrar?: string;
  /** Where the registration happens. */
  registeredAt: Location;
  /** The plugin that found it. */
  plugin: string;
}

/** One test that uses a symbol; "Tests that use it" lists them. */
export interface TestUse {
  /** The test's name. */
  name: string;
  /** Where the test sits. */
  location: Location;
}

/** A named group of units, chosen by people, by job. The overview draws one box per region. */
export interface Region {
  /** The region's ID: letters, digits, "_" and "-". */
  id: string;
  /** The region's name, by purpose. */
  name: string;
  /** An index from 0 to 8 into the colour-blind-safe palette. */
  color?: number;
  /** The IDs of the units in the region. */
  containers: string[];
  /** What the region does, in the inline subset. */
  description?: string;
  /** How to read the region. */
  readingTip?: string;
  /** The provenance mark of drafted text. */
  drafted?: Provenance;
}

/** A view that a link, a tour step or the history can open. */
export type View =
  | { view: "overview" }
  | { view: "region"; id: string }
  | { view: "unit"; id: string }
  | { view: "symbol"; id: string }
  | { view: "trace"; entryPoint: string }
  | { view: "trace"; symbol: string };

/** One step of the guided tour: a title, its text, and the view it opens. */
export interface TourStep {
  /** The step's title. */
  title: string;
  /** The step's text, in the inline subset. */
  text: string;
  /** The view that the step opens. */
  open: View;
  /** The provenance mark of drafted text. */
  drafted?: Provenance;
}

/** The project facts that acu adds from atlas.yaml. */
export interface Project {
  /** The project's name, shown in the header. */
  name: string;
  /** "owner/name" on the code host. */
  repo?: string;
  /** URL templates for source links, with {repo}, {commit}, {path}, {line} and {endLine}. */
  links?: { file?: string; line?: string; range?: string; folder?: string };
  /** The overview text, in the inline subset. */
  description?: string;
  /** The provenance mark of a drafted description. */
  descriptionDrafted?: Provenance;
  /** Rules that turn plain text in docs into links, such as issue numbers. */
  autolinks?: { pattern: string; url: string }[];
  /** Notes shown beside matching entry points. */
  entryPointNotes?: { kind?: string; pathPrefix?: string; title?: string; text: string }[];
  /** Path patterns that the adapters left out, named in the help text. */
  excluded?: string[];
}

/** One atlas.json: everything the viewer draws. */
export interface AtlasDoc {
  /** The data format version; the viewer refuses one it does not know. */
  version: number;
  /** The run that wrote the file: the tool, the commit, and each adapter with its notes. */
  meta: {
    schemaVersion: number;
    tool: { name: string; version: string };
    generated: string;
    commit?: string;
    languages: string[];
    adapters: { name: string; version: string; language: string; root?: string; notes?: string[] }[];
  };
  /** The project facts. */
  project?: Project;
  /** Every container. */
  containers: Container[];
  /** Every symbol. */
  symbols: Symbol[];
  /** Every relation. */
  edges: Edge[];
  /** The ways into the code. */
  entryPoints?: EntryPoint[];
  /** The tests of each symbol, by symbol ID. */
  tests?: Record<string, TestUse[]>;
  /** The regions of the overview. */
  regions?: Region[];
  /** Region IDs row by row, top to bottom. */
  layout?: { rows: string[][] };
  /** The guided tour. */
  tour?: TourStep[];
}
