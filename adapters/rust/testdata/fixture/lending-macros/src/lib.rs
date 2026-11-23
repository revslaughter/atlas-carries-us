//! Derive macros for the lending library.

use proc_macro::TokenStream;

/// Derives a `describe` function that names the type. The fixture's version
/// writes nothing.
#[proc_macro_derive(Describe)]
pub fn describe(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}
