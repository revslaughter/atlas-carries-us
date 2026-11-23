//! The Rust adapter of Atlas Carries Us: reads a Cargo workspace with
//! rustdoc's JSON output and rust-analyzer's SCIP index, and writes the data
//! format of `docs/schema.md`.
//!
//! [`extract::run`] does the work; `main.rs` reads the request and writes the
//! document. `docs/adapters.md` records the decisions and the limits.

pub mod docs;
pub mod extract;
pub mod glob;
pub mod lexer;
pub mod plugins;
pub mod request;
pub mod schema;
pub mod scip;
pub mod toolchain;
pub mod types;
