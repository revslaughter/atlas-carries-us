//! A stand-in for clap: the derive traits and macros, without a parser.

pub use clap_derive::{Args, Parser, Subcommand};

/// A type that parses the command line.
pub trait Parser: Sized {
    /// Parses the process's arguments; the stand-in never returns.
    fn parse() -> Self {
        unimplemented!("the stand-in clap parses nothing")
    }
}

/// An enum whose variants name subcommands.
pub trait Subcommand {}

/// A struct whose fields name arguments.
pub trait Args {}
