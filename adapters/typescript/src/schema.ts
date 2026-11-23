// The TypeScript form of the Atlas Carries Us data format, version 1, as
// this adapter writes it. docs/schema.md in the repository root explains
// every field; schema/atlas.schema.json holds the same rules for machines.
// The Go adapter (adapters/go/internal/schema) and the viewer
// (viewer/src/types.ts) hold their own copies; a change touches all of them.

/** The data format version that this adapter writes. */
export const VERSION = 1;

/** One atlas.json file, or the part of it that an adapter writes. The acu command adds the project, regions, layout and tour. */
export interface Document {
  /** The data format version: always {@link VERSION}. */
  version: number;
  /** The run that wrote the document. */
  meta: Meta;
  /** The packages, folders and files, sorted by ID. */
  containers: Container[];
  /** Every declared thing, sorted by ID. */
  symbols: Symbol[];
  /** The relations, each from the user to the thing used. */
  edges: Edge[];
  /** The ways into the code that the plugins found. */
  entryPoints?: EntryPoint[];
  /** The tests that use each symbol, keyed by the symbol's ID. */
  tests?: Record<string, TestUse[]>;
}

/** Describes the run that produced the document. */
export interface Meta {
  /** The data format version again, for readers that look here first. */
  schemaVersion: number;
  /** The program that wrote the final file. */
  tool: { name: string; version: string };
  /** The time of the run, RFC 3339, in UTC. */
  generated: string;
  /** The commit that the adapters read, when known. */
  commit?: string;
  /** The language tags in the document, such as "ts". */
  languages: string[];
  /** One entry per adapter run. */
  adapters: AdapterInfo[];
}

/** Describes one adapter run. */
export interface AdapterInfo {
  /** The adapter's name as atlas.yaml writes it, such as "typescript". */
  name: string;
  /** The adapter's release. */
  version: string;
  /** The language tag that starts the run's IDs. */
  language: string;
  /** The folder that the adapter read, relative to the repository root. */
  root?: string;
  /** The limits of the run in plain words; the viewer's help shows them. */
  notes?: string[];
}

/** Points at lines in a file. The path is relative to the repository root. */
export interface Location {
  /** The file's path from the repository root, with forward slashes. */
  path: string;
  /** The first line, counting from 1. */
  line: number;
  /** The last line, when the location spans more than one. */
  endLine?: number;
}

/** One block of a doc description: a paragraph, heading, list, code block or quote. */
export type Block =
  | { type: "paragraph"; text: string }
  | { type: "heading"; level: number; text: string }
  | { type: "list"; ordered: boolean; items: string[] }
  | { type: "code"; code: string; language?: string }
  | { type: "quote"; text: string };

/** A doc comment as written and in normal form. The viewer reads only the normal form. */
export interface Doc {
  /** The comment as written, without comment markers. */
  raw: string;
  /** The comment style: "tsdoc" when it uses a tag that only TSDoc defines, else "jsdoc". */
  format: "jsdoc" | "tsdoc";
  /** The first sentence, in the inline subset. The map draws it on every box. */
  summary: string;
  /** The whole comment as a list of blocks. */
  description: Block[];
  /** Each parameter's text, from `@param`, keyed by its name. */
  params?: Record<string, string>;
  /** Each type parameter's text, from `@typeParam` or `@template`. */
  typeParams?: Record<string, string>;
  /** The text about the result, from `@returns`. */
  returns?: string;
  /** What the code throws and when, from `@throws`. */
  throws?: { type?: string; text: string }[];
  /** Code examples, from `@example`. */
  examples?: { title?: string; code: string; language?: string }[];
  /** Related things, from `@see`. */
  seeAlso?: string[];
  /** The deprecation note; an empty string still marks the item deprecated. */
  deprecated?: string;
  /** The version that added the item, from `@since`. */
  since?: string;
  /** The block tags that have no field of their own, such as `@beta`. */
  tags?: { name: string; text: string }[];
}

/** A module, package, namespace, directory or file. A TypeScript tree runs package, folder, file. */
export interface Container {
  /** The container's ID, such as "ts:dir:immer/src/core". */
  id: string;
  /** The neutral kind. */
  kind: "module" | "package" | "namespace" | "directory" | "file";
  /** The language's own word, such as "folder" or "module", which the viewer shows. */
  languageKind: string;
  /** A short display name, such as "src/core". */
  name: string;
  /** The container's path from the repository root. */
  path?: string;
  /** The enclosing container. */
  parent?: string;
  /** The language tag. */
  language?: string;
  /** Marks a container that the overview draws as a box: a folder that holds source files. */
  unit?: boolean;
  /** The container's doc: a file's comment, or a folder's doc from its index file. */
  doc?: Doc;
  /** The outside packages that the container imports, by package name. */
  externalImports?: string[];
}

/** One parameter or result of a callable. */
export interface Param {
  /** The parameter's name; a result has none. */
  name?: string;
  /** The type as a reader sees it: as written, or as the checker infers it. */
  type: string;
  /** The IDs of the symbols that the type names, so the viewer can link them. */
  refs?: string[];
  /** Marks a parameter that a caller may leave out. */
  optional?: boolean;
  /** Marks a rest parameter (`...args`). */
  rest?: boolean;
  /** The default value, as written. */
  default?: string;
}

/** Describes a callable: its parameters, results and type parameters. */
export interface Signature {
  /** The type parameters as written, such as "<T extends object>". */
  typeParams?: string;
  /** The parameters in order. */
  params: Param[];
  /** The result; a function that returns nothing has none. */
  results: Param[];
  /** The whole signature as the code writes it. */
  text?: string;
}

/** A type as a reader sees it, with the symbols it names. */
export interface TypeText {
  /** The type as written. */
  text: string;
  /** The IDs of the symbols that the type names. */
  refs?: string[];
}

/** One declared thing: a type, a callable, a member, a constant or a variable. */
export interface Symbol {
  /** The symbol's ID: the module, "#", and the symbol's path, such as "ts:immer/src/core/immerClass#Immer.produce". */
  id: string;
  /** The neutral kind. */
  kind: SymbolKind;
  /** The language's own word, such as "arrow function", which the viewer shows. */
  languageKind: string;
  /** The declared name. */
  name: string;
  /** The owning symbol, or the file of a top-level symbol. */
  parent: string;
  /** The language tag. */
  language?: string;
  /** Reports whether code outside the module may use the symbol. */
  exported: boolean;
  /** Where the declaration sits. */
  location: Location;
  /** The doc comment. */
  doc?: Doc;
  /** A generic type's type parameters, as written. */
  typeParams?: string;
  /** The signature of a function, method or constructor. */
  signature?: Signature;
  /** The type of a property, constant or variable, or the type that an alias names. */
  type?: TypeText;
  /** A constant's value, cut to 80 characters. */
  value?: string;
  /** Joins the declarations of one statement; the key stays unique in a file. */
  group?: string;
  /** Words such as "static", "async", "readonly" and "optional". */
  modifiers?: string[];
  /** Decorators, as written. */
  annotations?: string[];
  /** Well-known outside interfaces that the symbol satisfies. */
  satisfies?: string[];
}

/** The neutral symbol kinds that the schema defines. */
export type SymbolKind =
  | "class"
  | "struct"
  | "interface"
  | "trait"
  | "enum"
  | "type"
  | "type_alias"
  | "function"
  | "method"
  | "constructor"
  | "field"
  | "property"
  | "constant"
  | "variable"
  | "event"
  | "macro"
  | "namespace";

/** The relations that the schema defines. Each one points from the user to the thing used. */
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
  /** "exact" when the type checker resolved the edge; "heuristic" for an edge that a framework's file convention implies, such as a layout around a page. */
  confidence: "exact" | "heuristic";
  /** The first line of the use, in the file of `from`. */
  line?: number;
  /** How many places make the same use, when more than one. */
  count?: number;
}

/** One way into the code: an HTTP route, a CLI command, and so on. */
export interface EntryPoint {
  /** The entry point's ID, by kind and name, such as "http:GET /books/:id". */
  id: string;
  /** The kind of entry point. */
  kind: "http" | "cli" | "message" | "job" | "grpc";
  /** The route or command as a reader writes it. */
  name: string;
  /** An HTTP route's method. */
  method?: string;
  /** An HTTP route's path, with every mount's prefix. */
  path?: string;
  /** The function or method that serves the entry point. */
  handler: string;
  /** Marks a handler written inline inside a function; `handler` then names that function. */
  inline?: boolean;
  /** The handler, when it comes from outside the code, such as "serveStatic(…)". */
  externalHandler?: string;
  /** The function that registers the entry point. */
  registrar?: string;
  /** Where the registration happens. */
  registeredAt: Location;
  /** The plugin that found the entry point: "express", "hono" or "sveltekit". */
  plugin: string;
}

/** One test that uses a symbol. */
export interface TestUse {
  /** The test's titles, joined with " > ", such as "HttpClient > borrow > refuses …". */
  name: string;
  /** Where the test's call to it() or test() sits. */
  location: Location;
}
