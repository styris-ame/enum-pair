use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Path};

use crate::parse::{Input, Member, Pair};

pub fn generate(input: &Input) -> TokenStream {
    input.pairs.iter().map(|pair| self::pair(&input.id, &input.krate, pair)).collect()
}

fn pair(id: &Ident, krate: &Path, pair: &Pair) -> TokenStream {
    let each_type = pair.each_type.as_ref().map(|each_type| each_type_macro(id, pair, each_type));
    if pair.types_only {
        return quote!(#each_type);
    }

    let (name, vis, members) = (&pair.name, &pair.vis, &pair.members);
    let doc = (!pair.attrs.iter().any(|attr| attr.path().is_ident("doc")))
        .then(|| format!("The data paired with each [`{id}`] variant."))
        .into_iter();
    let derives = &pair.derives;
    let derive = (!derives.is_empty()).then(|| quote!(#[derive(#(#derives),*)]));
    let attrs = &pair.attrs;
    let variants = members.iter().map(|member| {
        let (variant, ty, attrs) = (&member.variant, &member.ty, &member.attrs);
        quote!(#(#[#attrs])* #variant(#ty))
    });
    let pairs = members.iter().map(|member| {
        let (variant, rest) = (&member.variant, &member.rest);
        quote!((Self::#variant #rest, #name::#variant(_)))
    });
    let conversions = members.iter().filter(|member| member.unique).map(|member| conversions(krate, name, member));
    let each = pair.each.as_ref().map(|each| each_macro(pair, each));

    quote! {
        #(#[doc = #doc])*
        #derive
        #(#[#attrs])*
        #vis enum #name {
            #(#variants,)*
        }

        #[automatically_derived]
        impl #krate::Paired<#name> for #id {
            #[allow(unreachable_patterns)]
            fn pairs_with(&self, data: &#name) -> bool {
                ::core::matches!((self, data), #(#pairs)|*)
            }
        }

        #[automatically_derived]
        impl #krate::PairedData for #name {
            type Id = #id;
        }

        #(#conversions)*

        #each

        #each_type
    }
}

fn conversions(krate: &Path, name: &Ident, member: &Member) -> TokenStream {
    let (variant, ty) = (&member.variant, &member.ty);
    quote! {
        #[automatically_derived]
        impl ::core::convert::From<#ty> for #name {
            fn from(value: #ty) -> Self {
                Self::#variant(value)
            }
        }

        #[automatically_derived]
        #[allow(unreachable_patterns)]
        impl ::core::convert::TryFrom<#name> for #ty {
            type Error = #name;

            fn try_from(data: #name) -> ::core::result::Result<Self, #name> {
                match data {
                    #name::#variant(value) => ::core::result::Result::Ok(value),
                    other => ::core::result::Result::Err(other),
                }
            }
        }

        #[automatically_derived]
        #[allow(unreachable_patterns)]
        impl #krate::Variant<#name> for #ty {
            fn from_ref(data: &#name) -> ::core::option::Option<&Self> {
                match data {
                    #name::#variant(value) => ::core::option::Option::Some(value),
                    _ => ::core::option::Option::None,
                }
            }

            fn from_mut(data: &mut #name) -> ::core::option::Option<&mut Self> {
                match data {
                    #name::#variant(value) => ::core::option::Option::Some(value),
                    _ => ::core::option::Option::None,
                }
            }
        }
    }
}

fn each_macro(pair: &Pair, each: &Ident) -> TokenStream {
    let name = &pair.name;
    let arms = pair.members.iter().map(|member| {
        let variant = &member.variant;
        quote!(#name::#variant($bind) => $body)
    });
    let doc = format!("Runs the body for whichever [`{name}`] variant the data holds, with its value bound to the pattern.");
    match_macro(each, &doc, &quote!($bind:pat), arms)
}

fn each_type_macro(id: &Ident, pair: &Pair, each_type: &Ident) -> TokenStream {
    let wrap = pair.optional.then(|| quote!(::core::option::Option::Some));
    let arms = pair.members.iter().map(|member| {
        let (variant, rest, ty) = (&member.variant, &member.rest, &member.ty);
        quote!(#id::#variant #rest => #wrap({ #[allow(dead_code)] type $T = #ty; $body }))
    });
    let fallback = pair.optional.then(|| quote!(#[allow(unreachable_patterns)] _ => ::core::option::Option::None));
    let name = &pair.name;
    let doc = format!("Runs the body with `T` as the type the [`{id}`] variant pairs with in `{name}`.");
    let doc = if pair.optional {
        doc + " Gives `None` for variants without one."
    } else {
        doc
    };
    match_macro(each_type, &doc, &quote!($T:ident), arms.chain(fallback))
}

/// A crate-visible `macro_rules!` taking `(value, binding => body)` that matches on the value with the given arms
fn match_macro(name: &Ident, doc: &str, binding: &TokenStream, arms: impl Iterator<Item = TokenStream>) -> TokenStream {
    quote! {
        #[doc = #doc]
        #[allow(unused_macros)]
        macro_rules! #name {
            ($value:expr, #binding => $body:expr $(,)?) => {
                match $value {
                    #(#arms,)*
                }
            };
        }

        #[allow(unused_imports)]
        pub(crate) use #name;
    }
}
