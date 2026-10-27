//! The derive behind `enum_pair`. Use that crate instead, it documents everything this generates.

mod generate;
mod parse;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

#[proc_macro_derive(EnumPair, attributes(pair))]
pub fn derive_enum_pair(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    parse::parse(input).map_or_else(syn::Error::into_compile_error, |input| generate::generate(&input)).into()
}
