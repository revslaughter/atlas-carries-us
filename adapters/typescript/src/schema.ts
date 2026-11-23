// The TypeScript form of the Atlas Carries Us data format, version 1, as
// this adapter writes it. docs/schema.md in the repository root explains
// every field; schema/atlas.schema.json holds the same rules for machines.

/** The data format version that this adapter writes. */
export const VERSION = 1;

/** One atlas.json file, or the part of it that an adapter writes. */
export interface Document {
  version: number;
  meta: Meta;
  containers: Container[];
  symbols: Symbol[];
  edges: Edge[];
  entryPoints?: EntryPoint[];
  tests?: Record<string, TestUse[]>;
}

/** Describes the run that produced the document. */
export interface Meta {
  schemaVersion: number;
  tool: { name: string; version: string };
  generated: string;
  commit?: string;
  languages: string[];
  adapters: AdapterInfo[];
}

/** Describes one adapter run. */
export interface AdapterInfo {
  name: string;
  version: string;
  language: string;
  root?: string;
  notes?: string[];
}

/** Points at lines in a file. The path is relative to the repository root. */
export interface Location {
  path: string;
  line: number;
  endLine?: number;
}

/** One block of a doc description: a paragraph, heading, list, code block or quote. */
export type Block =
  | { type: "paragraph"; text: string }
  | { type: "heading"; level: number; text: string }
  | { type: "list"; ordered: boolean; items: string[] }
  | { type: "code"; code: string; language?: string }
  | { type: "quote"; text: string };

/** A doc comment as written and in normal form. */
export interface Doc {
  raw: string;
  format: "jsdoc" | "tsdoc";
  summary: string;
  description: Block[];
  params?: Record<string, string>;
  typeParams?: Record<string, string>;
  returns?: string;
  throws?: { type?: string; text: string }[];
  examples?: { title?: string; code: string; language?: string }[];
  seeAlso?: string[];
  deprecated?: string;
  since?: string;
  tags?: { name: string; text: string }[];
}

/** A module, package, namespace, directory or file. */
export interface Container {
  id: string;
  kind: "module" | "package" | "namespace" | "directory" | "file";
  languageKind: string;
  name: string;
  path?: string;
  parent?: string;
  language?: string;
  unit?: boolean;
  doc?: Doc;
  externalImports?: string[];
}

/** One parameter or result of a callable. */
export interface Param {
  name?: string;
  type: string;
  refs?: string[];
  optional?: boolean;
  rest?: boolean;
  default?: string;
}

/** Describes a callable. */
export interface Signature {
  typeParams?: string;
  params: Param[];
  results: Param[];
  text?: string;
}

/** A type as a reader sees it, with the symbols it names. */
export interface TypeText {
  text: string;
  refs?: string[];
}

/** One declared thing: a type, a callable, a member, a constant or a variable. */
export interface Symbol {
  id: string;
  kind: SymbolKind;
  languageKind: string;
  name: string;
  parent: string;
  language?: string;
  exported: boolean;
  location: Location;
  doc?: Doc;
  typeParams?: string;
  signature?: Signature;
  type?: TypeText;
  value?: string;
  group?: string;
  modifiers?: string[];
  annotations?: string[];
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

/** One relation: from uses to. */
export interface Edge {
  from: string;
  to: string;
  kind: EdgeKind;
  confidence: "exact" | "heuristic";
  line?: number;
  count?: number;
}

/** One way into the code: an HTTP route, a CLI command, and so on. */
export interface EntryPoint {
  id: string;
  kind: "http" | "cli" | "message" | "job" | "grpc";
  name: string;
  method?: string;
  path?: string;
  handler: string;
  inline?: boolean;
  externalHandler?: string;
  registrar?: string;
  registeredAt: Location;
  plugin: string;
}

/** One test that uses a symbol. */
export interface TestUse {
  name: string;
  location: Location;
}
