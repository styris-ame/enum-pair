use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Attribute, Data, DeriveInput, Error, Fields, Ident, Meta, Path, Result, Token, Type, Visibility, meta::ParseNestedMeta,
    parenthesized, parse::ParseStream, punctuated::Punctuated, token::Paren,
};

pub struct Input {
    pub id: Ident,
    pub krate: Path,
    pub pairs: Vec<Pair>,
}

pub struct Pair {
    pub name: Ident,
    pub vis: Visibility,
    pub derives: Vec<Path>,
    pub attrs: Vec<Meta>,
    pub each: Option<Ident>,
    pub each_type: Option<Ident>,
    /// Variants may leave this pair out
    pub optional: bool,
    /// Only maps variants to types, without generating an enum
    pub types_only: bool,
    pub members: Vec<Member>,
}

/// An id variant and the type it's paired with
pub struct Member {
    pub variant: Ident,
    /// Matches the id variant whatever its fields are
    pub rest: TokenStream,
    pub ty: Type,
    pub attrs: Vec<Meta>,
    /// Whether no other member of the pair has the same type, so conversions can't be ambiguous
    pub unique: bool,
}

pub fn parse(input: DeriveInput) -> Result<Input> {
    let Data::Enum(data) = input.data else {
        return Err(Error::new(input.ident.span(), "EnumPair can only be derived for enums"));
    };
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        return Err(Error::new_spanned(&input.generics, "EnumPair doesn't support generic enums"));
    }
    if data.variants.is_empty() {
        return Err(Error::new(input.ident.span(), "EnumPair needs at least one variant"));
    }

    let mut krate = None;
    let mut pairs = Vec::new();
    for attr in input.attrs.iter().filter(|attr| attr.path().is_ident("pair")) {
        let pair = pair(attr, &input.vis, &mut krate)?;
        if pair.name == input.ident {
            return Err(Error::new(pair.name.span(), "a pair needs a different name than the id enum"));
        }
        if pairs.iter().any(|other: &Pair| other.name == pair.name) {
            return Err(Error::new(pair.name.span(), "this pair is declared more than once"));
        }
        pairs.push(pair);
    }
    if pairs.is_empty() {
        return Err(Error::new(input.ident.span(), "missing `#[pair(name = ...)]` naming the data enum to generate"));
    }
    let macros: Vec<&Ident> = pairs.iter().flat_map(|pair| pair.each.iter().chain(&pair.each_type)).collect();
    if let Some(clash) = macros.iter().enumerate().find_map(|(i, name)| macros[..i].contains(name).then_some(name)) {
        return Err(Error::new(clash.span(), "another generated macro already has this name"));
    }

    let mut errors = Errors::default();
    for variant in &data.variants {
        errors.check(members(variant, &mut pairs));
    }
    errors.finish()?;
    for pair in &pairs {
        if pair.members.is_empty() {
            errors.push(Error::new(pair.name.span(), "no variant is paired with this"));
        }
    }
    errors.finish()?;

    for pair in &mut pairs {
        let types: Vec<_> = pair.members.iter().map(|member| member.ty.to_token_stream().to_string()).collect();
        for (member, ty) in pair.members.iter_mut().zip(&types) {
            member.unique = types.iter().filter(|other| *other == ty).count() == 1;
        }
    }

    Ok(Input {
        id: input.ident,
        krate: krate.unwrap_or_else(|| syn::parse_quote!(::enum_pair)),
        pairs,
    })
}

fn pair(attr: &Attribute, id_vis: &Visibility, krate: &mut Option<Path>) -> Result<Pair> {
    let mut name = None;
    let mut vis: Option<Visibility> = None;
    let mut derives = Vec::new();
    let mut attrs = Vec::new();
    let mut each = None;
    let mut each_type = None;
    let mut optional = false;
    let mut types_only = false;
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("name") {
            once(&meta, &mut name, meta.value()?.parse()?)
        } else if meta.path.is_ident("vis") {
            once(&meta, &mut vis, meta.value()?.parse()?)
        } else if meta.path.is_ident("derive") {
            derives.extend(list::<Path>(meta.input)?);
            Ok(())
        } else if meta.path.is_ident("attrs") {
            attrs.extend(list::<Meta>(meta.input)?);
            Ok(())
        } else if meta.path.is_ident("each") {
            once(&meta, &mut each, meta.value()?.parse()?)
        } else if meta.path.is_ident("each_type") {
            once(&meta, &mut each_type, meta.value()?.parse()?)
        } else if meta.path.is_ident("optional") {
            optional = true;
            Ok(())
        } else if meta.path.is_ident("types_only") {
            types_only = true;
            Ok(())
        } else if meta.path.is_ident("crate") {
            once(&meta, krate, meta.value()?.parse()?)
        } else {
            Err(meta.error("expected `name`, `vis`, `derive`, `attrs`, `each`, `each_type`, `optional`, `types_only` or `crate`"))
        }
    })?;
    let name: Ident = name.ok_or_else(|| Error::new_spanned(attr, "missing `name = ...` for the pair"))?;
    if types_only {
        if vis.is_some() || !derives.is_empty() || !attrs.is_empty() || each.is_some() {
            return Err(Error::new_spanned(attr, "`vis`, `derive`, `attrs` and `each` need an enum, which `types_only` skips"));
        }
        if each_type.is_none() {
            return Err(Error::new_spanned(attr, "`types_only` does nothing without `each_type`"));
        }
    }
    Ok(Pair {
        name,
        vis: vis.unwrap_or_else(|| id_vis.clone()),
        derives,
        attrs,
        each,
        each_type,
        optional,
        types_only,
        members: Vec::new(),
    })
}

/// Adds this variant to each pair it names, and checks that it names every pair it has to
fn members(variant: &syn::Variant, pairs: &mut [Pair]) -> Result<()> {
    let rest = match variant.fields {
        Fields::Named(_) => quote!({ .. }),
        Fields::Unnamed(_) => quote!((..)),
        Fields::Unit => TokenStream::new(),
    };
    let names: Vec<Ident> = pairs.iter().map(|pair| pair.name.clone()).collect();
    let mut found = vec![false; pairs.len()];
    for attr in variant.attrs.iter().filter(|attr| attr.path().is_ident("pair")) {
        for Entry {
            pair,
            ty,
            attrs,
        } in attr.parse_args_with(|input: ParseStream| entries(input, &names))?
        {
            if std::mem::replace(&mut found[pair], true) {
                return Err(Error::new_spanned(attr, format!("this variant is paired with `{}` more than once", names[pair])));
            }
            pairs[pair].members.push(Member {
                variant: variant.ident.clone(),
                rest: rest.clone(),
                ty,
                attrs,
                unique: false,
            });
        }
    }

    let missing: Vec<_> = pairs.iter().zip(&found).filter(|(pair, found)| !pair.optional && !**found).collect();
    match missing.as_slice() {
        [] => Ok(()),
        [_] if pairs.len() == 1 => {
            Err(Error::new(variant.ident.span(), "missing `#[pair(Type)]` naming the data this variant pairs with"))
        }
        missing => {
            let names: Vec<_> = missing.iter().map(|(pair, _)| format!("`{} = Type`", pair.name)).collect();
            Err(Error::new(variant.ident.span(), format!("missing `#[pair(...)]` entries: {}", names.join(", "))))
        }
    }
}

/// One variant's type for one pair
struct Entry {
    pair: usize,
    ty: Type,
    attrs: Vec<Meta>,
}

/// Parses `Type, attrs(...)` when there's only one pair, or `Name = Type, Other(Type, attrs(...))` otherwise
fn entries(input: ParseStream, names: &[Ident]) -> Result<Vec<Entry>> {
    let pair_at = |input: ParseStream| {
        let ident = input.fork().parse::<Ident>().ok()?;
        (input.peek2(Token![=]) || input.peek2(Paren)).then(|| names.iter().position(|pair| *pair == ident)).flatten()
    };
    if pair_at(input).is_none() {
        if input.peek(syn::Ident) && input.peek2(Token![=]) {
            return Err(input.error("no pair on the enum has this name"));
        }
        if names.len() > 1 {
            return Err(input.error("name the pair this is for, like `Name = Type`"));
        }
        return Ok(vec![typed(input, 0)?]);
    }

    let mut entries = Vec::new();
    while !input.is_empty() {
        let Some(pair) = pair_at(input) else {
            if input.fork().parse::<Ident>().is_ok_and(|ident| ident == "attrs") {
                return Err(input.error("with names, attributes go inside the entry, like `Name(Type, attrs(...))`"));
            }
            return Err(input.error("expected `Name = Type` or `Name(Type, attrs(...))` for a pair on the enum"));
        };
        input.parse::<Ident>()?;
        if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            entries.push(Entry {
                pair,
                ty: input.parse()?,
                attrs: Vec::new(),
            });
        } else {
            let content;
            parenthesized!(content in input);
            entries.push(typed(&content, pair)?);
        }
        if !input.is_empty() {
            input.parse::<Token![,]>()?;
        }
    }
    Ok(entries)
}

/// Parses `Type` with an optional `, attrs(...)` after it
fn typed(input: ParseStream, pair: usize) -> Result<Entry> {
    let ty = input.parse()?;
    let mut attrs = Vec::new();
    while !input.is_empty() {
        input.parse::<Token![,]>()?;
        if input.is_empty() {
            break;
        }
        let key: Ident = input.parse()?;
        if key != "attrs" {
            return Err(Error::new(key.span(), "expected `attrs(...)`"));
        }
        attrs.extend(list::<Meta>(input)?);
    }
    Ok(Entry {
        pair,
        ty,
        attrs,
    })
}

fn once<T>(meta: &ParseNestedMeta, slot: &mut Option<T>, value: T) -> Result<()> {
    if slot.replace(value).is_some() {
        return Err(meta.error("this option is given more than once"));
    }
    Ok(())
}

fn list<T: syn::parse::Parse>(input: ParseStream) -> Result<Punctuated<T, Token![,]>> {
    let content;
    parenthesized!(content in input);
    content.parse_terminated(T::parse, Token![,])
}

/// Collects every error so they all show up in one build
#[derive(Default)]
struct Errors(Option<Error>);

impl Errors {
    fn push(&mut self, error: Error) {
        match &mut self.0 {
            Some(errors) => errors.combine(error),
            None => self.0 = Some(error),
        }
    }

    fn check(&mut self, result: Result<()>) {
        if let Err(error) = result {
            self.push(error);
        }
    }

    fn finish(&mut self) -> Result<()> {
        self.0.take().map_or(Ok(()), Err)
    }
}
