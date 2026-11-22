//! The project config file, atlas.yaml. docs/config.md explains each field;
//! schema/config.schema.json holds the same rules for machines.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;

pub const CONFIG_SCHEMA: &str = include_str!("../../schema/config.schema.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)] // `schema` and `version` exist for editors and the schema check.
pub struct Config {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    pub version: u32,
    pub project: Project,
    pub languages: Vec<Language>,
    #[serde(default)]
    pub entry_points: Vec<EntryPointPlugin>,
    #[serde(default)]
    pub regions: Vec<Region>,
    #[serde(default)]
    pub layout: Option<Layout>,
    #[serde(default)]
    pub tour: Vec<TourStep>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Project {
    pub name: String,
    pub repo: Option<String>,
    pub links: Option<Links>,
    pub description: Option<String>,
    pub description_drafted: Option<Value>,
    #[serde(default)]
    pub autolinks: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Links {
    Preset(String),
    Templates(serde_json::Map<String, Value>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Language {
    pub adapter: String,
    pub root: Option<String>,
    pub command: Option<Vec<String>>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub tests: Vec<String>,
    pub options: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryPointPlugin {
    pub plugin: String,
    pub adapter: Option<String>,
    pub options: Option<Value>,
    #[serde(default)]
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Note {
    pub path_prefix: Option<String>,
    pub title: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Region {
    pub id: String,
    pub name: String,
    pub color: Option<u8>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub default: bool,
    pub description: Option<String>,
    pub reading_tip: Option<String>,
    pub drafted: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    #[serde(default)]
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TourStep {
    pub title: String,
    pub text: String,
    pub open: Open,
    pub drafted: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Open {
    pub view: String,
    pub id: Option<String>,
    pub path: Option<String>,
    pub name: Option<String>,
    pub entry_point: Option<String>,
    pub symbol: Option<String>,
}

/// A loaded config and the folder it came from. Relative paths in the config
/// resolve against that folder.
pub struct Loaded {
    pub config: Config,
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
