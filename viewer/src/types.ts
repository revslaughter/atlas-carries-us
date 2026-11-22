// Schema v1, as the viewer reads it. docs/schema.md explains every field.

export interface Location {
  path: string;
  line: number;
  endLine?: number;
}
export interface Provenance {
  by: string;
  at: string;
}

export type Block =
  | { type: "paragraph"; text: string }
  | { type: "heading"; level: number; text: string }
  | { type: "list"; ordered: boolean; items: string[] }
  | { type: "code"; code: string; language?: string }
  | { type: "quote"; text: string };

export interface Doc {
  raw: string;
  format: string;
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

export interface Param {
  name?: string;
  type: string;
  refs?: string[];
  optional?: boolean;
  rest?: boolean;
  default?: string;
}
export interface Signature {
  typeParams?: string;
  params: Param[];
  results: Param[];
  text?: string;
}
export interface TypeText {
  text: string;
  refs?: string[];
}

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

export interface Symbol {
  id: string;
  kind: string;
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

export type EdgeKind =
  "calls" | "implements" | "extends" | "field_type" | "param_type" | "return_type" | "imports" | "overrides";
export interface Edge {
  from: string;
  to: string;
  kind: EdgeKind;
  confidence: "exact" | "heuristic";
  line?: number;
  count?: number;
}

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

export interface TestUse {
  name: string;
  location: Location;
}

export interface Region {
  id: string;
  name: string;
  color?: number;
  containers: string[];
  description?: string;
  readingTip?: string;
  drafted?: Provenance;
}

export type View =
  | { view: "overview" }
  | { view: "region"; id: string }
  | { view: "unit"; id: string }
  | { view: "symbol"; id: string }
  | { view: "trace"; entryPoint: string }
  | { view: "trace"; symbol: string };

export interface TourStep {
  title: string;
  text: string;
  open: View;
  drafted?: Provenance;
}

export interface Project {
  name: string;
  repo?: string;
  links?: { file?: string; line?: string; range?: string; folder?: string };
  description?: string;
  descriptionDrafted?: Provenance;
  autolinks?: { pattern: string; url: string }[];
  entryPointNotes?: { kind?: string; pathPrefix?: string; title?: string; text: string }[];
  excluded?: string[];
}

export interface AtlasDoc {
  version: number;
  meta: {
    schemaVersion: number;
    tool: { name: string; version: string };
    generated: string;
    commit?: string;
    languages: string[];
    adapters: { name: string; version: string; language: string; root?: string; notes?: string[] }[];
  };
  project?: Project;
  containers: Container[];
  symbols: Symbol[];
  edges: Edge[];
  entryPoints?: EntryPoint[];
  tests?: Record<string, TestUse[]>;
  regions?: Region[];
  layout?: { rows: string[][] };
  tour?: TourStep[];
}
