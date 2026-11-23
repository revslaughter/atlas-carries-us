//! The stand-in clap's derive macros. Each one implements the matching trait
//! for the type it decorates, and reads no attributes.

use proc_macro::{TokenStream, TokenTree};

/// Finds the name of the struct or enum in a derive's input.
fn type_name(input: TokenStream) -> String {
    let mut take = false;
    for tt in input {
        if let TokenTree::Ident(id) = tt {
            let s = id.to_string();
            if take {
                return s;
            }
            take = s == "struct" || s == "enum";
        }
    }
    panic!("the stand-in derive found no type name");
}

/// Writes `impl clap::<trait> for <type> {}` for the decorated type.
fn implement(trait_name: &str, input: TokenStream) -> TokenStream {
    format!("#[automatically_derived] impl ::clap::{trait_name} for {} {{}}", type_name(input))
        .parse()
        .unwrap()
}

/// Derives the stand-in `Parser`.
#[proc_macro_derive(Parser, attributes(command, arg))]
pub fn parser(input: TokenStream) -> TokenStream {
    implement("Parser", input)
}

/// Derives the stand-in `Subcommand`.
#[proc_macro_derive(Subcommand, attributes(command, arg))]
pub fn subcommand(input: TokenStream) -> TokenStream {
    implement("Subcommand", input)
}

/// Derives the stand-in `Args`.
#[proc_macro_derive(Args, attributes(command, arg))]
pub fn args(input: TokenStream) -> TokenStream {
    implement("Args", input)
}
