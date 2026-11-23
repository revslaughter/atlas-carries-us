//! The project config file, atlas.yaml. docs/config.md explains each field;
//! schema/config.schema.json holds the same rules for machines.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;

/// The JSON Schema of atlas.yaml, built into acu.
pub const CONFIG_SCHEMA: &str = include_str!("../../schema/config.schema.json");

/// One atlas.yaml: the project, its languages, its plugins and the text that
/// people write around the facts.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)] // `schema` and `version` exist for editors and the schema check.
pub struct Config {
    /// The schema that an editor checks the file against.
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    /// The config format's version: 1.
    pub version: u32,
    /// The project's name, links and description.
    pub project: Project,
    /// The adapters to run, one per language or root.
    pub languages: Vec<Language>,
    /// The entry-point plugins to switch on.
    #[serde(default)]
    pub entry_points: Vec<EntryPointPlugin>,
    /// The regions of the overview.
    #[serde(default)]
    pub regions: Vec<Region>,
    /// Where the regions sit on the overview.
    #[serde(default)]
    pub layout: Option<Layout>,
    /// The guided tour.
    #[serde(default)]
    pub tour: Vec<TourStep>,
}

/// The project's name, code host, link templates and description.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Project {
    /// The name in the viewer's header.
    pub name: String,
    /// The repository as `owner/name` on the code host.
    pub repo: Option<String>,
    /// A link preset, or link templates of its own.
    pub links: Option<Links>,
    /// The overview text.
    pub description: Option<String>,
    /// The provenance mark of a description that a program drafted.
    pub description_drafted: Option<Value>,
    /// Rules that turn plain text in docs into links.
    #[serde(default)]
    pub autolinks: Vec<Value>,
}

/// How the viewer links to the source: a preset name or templates.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Links {
    /// A preset: "github", "gitlab" or "none".
    Preset(String),
    /// URL templates for files, lines, ranges and folders.
    Templates(serde_json::Map<String, Value>),
}

/// One adapter run: which adapter reads which folder, and how.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Language {
    /// The adapter's name, such as "go", "typescript" or "rust".
    pub adapter: String,
    /// The folder to read, relative to the config file.
    pub root: Option<String>,
    /// A program to run in place of the default adapter.
    pub command: Option<Vec<String>>,
    /// Globs of folders to keep.
    #[serde(default)]
    pub include: Vec<String>,
    /// Globs of folders to leave out.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Globs that mark test files.
    #[serde(default)]
    pub tests: Vec<String>,
    /// The options that only this adapter reads.
    pub options: Option<Value>,
}

/// One entry-point plugin to switch on.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryPointPlugin {
    /// The plugin's name, such as "go-chi" or "axum".
    pub plugin: String,
    /// The adapter that runs the plugin, when acu does not know it.
    pub adapter: Option<String>,
    /// The plugin's own options.
    pub options: Option<Value>,
    /// Notes that the viewer shows beside matching entry points.
    #[serde(default)]
    pub notes: Vec<Note>,
}

/// A note that the viewer shows beside the entry points whose path starts
/// with a prefix.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Note {
    /// The path prefix that the note applies to.
    pub path_prefix: Option<String>,
    /// The note's title.
    pub title: Option<String>,
    /// The note's text.
    pub text: String,
}

/// A named group of units on the overview, chosen by job.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Region {
    /// The region's ID: letters, digits, `_` and `-`.
    pub id: String,
    /// The region's name.
    pub name: String,
    /// An index into the viewer's palette, from 0 to 8.
    pub color: Option<u8>,
    /// Globs of unit paths that the region holds.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Marks the region that takes every unit no other region holds.
    #[serde(default)]
    pub default: bool,
    /// What the region does.
    pub description: Option<String>,
    /// How to read the region.
    pub reading_tip: Option<String>,
    /// The provenance mark of text that a program drafted.
    pub drafted: Option<Value>,
}

/// Where the regions sit on the overview.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// Region IDs, row by row, top to bottom.
    #[serde(default)]
    pub rows: Vec<Vec<String>>,
}

/// One step of the guided tour.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TourStep {
    /// The step's title.
    pub title: String,
    /// The step's text.
    pub text: String,
    /// The view that the step opens.
    pub open: Open,
    /// The provenance mark of text that a program drafted.
    pub drafted: Option<Value>,
}

/// The view that a tour step opens, with its target by ID or by name.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Open {
    /// The view: overview, region, unit, symbol or trace.
    pub view: String,
    /// An exact ID of a region, unit or symbol.
    pub id: Option<String>,
    /// A unit's path, relative to the repository root.
    pub path: Option<String>,
    /// A symbol's qualified name, such as `Immer.produce`.
    pub name: Option<String>,
    /// An entry point's ID or name, for a trace.
    pub entry_point: Option<String>,
    /// The function or method that a trace starts from.
    pub symbol: Option<String>,
}

/// A loaded config and the folder it came from. Relative paths in the config
/// resolve against that folder.
pub struct Loaded {
    /// The parsed config.
    pub config: Config,
    /// The folder that holds the config file.
    pub dir: PathBuf,
}

/// Reads atlas.yaml, checks it against the config schema, and parses it.
pub fn load(path: &Path) -> Result<Loaded> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let value: Value = serde_yaml_ng::from_str(&text)
        .with_context(|| format!("{} holds YAML that does not parse", path.display()))?;
    let problems = validate(&value, CONFIG_SCHEMA)?;
    if !problems.is_empty() {
        bail!(
            "{} breaks the config schema:\n  {}",
            path.display(),
            problems.join("\n  ")
        );
    }
    let config: Config = serde_json::from_value(value)
        .with_context(|| format!("read the fields of {}", path.display()))?;
    let dir = path
        .canonicalize()
        .with_context(|| format!("find {}", path.display()))?
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(Loaded { config, dir })
}

/// Validates a value against a JSON Schema and returns one line per problem.
pub fn validate(value: &Value, schema_text: &str) -> Result<Vec<String>> {
    let schema: Value = serde_json::from_str(schema_text).context("parse an embedded schema")?;
    let validator =
        jsonschema::validator_for(&schema).map_err(|e| anyhow::anyhow!("compile a schema: {e}"))?;
    let mut out: Vec<String> = validator
        .iter_errors(value)
        .map(|e| {
            let at = e.instance_path().to_string();
            format!(
                "{}: {}",
                if at.is_empty() {
                    "(top level)".into()
                } else {
                    at
                },
                e
            )
        })
        .collect();
    out.truncate(40);
    Ok(out)
}
