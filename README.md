# enum_pair

Pairs an id enum with a data enum, one variant to one variant, when the id variants carry fields of their own.

`strum`'s `EnumDiscriminants` and `kinded` derive a fieldless kind enum from a data enum. That falls apart once the id needs fields, like a user id or a page number, so here the id enum is the source and each variant names the data it stands for.

Say a cache holds a few kinds of assets, and some are looked up by more than a name:

```rust
use enum_pair::EnumPair;

#[derive(EnumPair, Clone, Copy, PartialEq, Eq, Hash)]
#[pair(name = Asset, derive(Debug), each = each_asset)]
pub enum AssetId {
    #[pair(Config)]
    Config,
    #[pair(Image)]
    Texture(u32),
    #[pair(Clip, attrs(doc = "A sound from one of the banks"))]
    Sound { bank: u8, index: u16 },
}
```

That generates the data enum

```rust
#[derive(Debug)]
pub enum Asset {
    Config(Config),
    Texture(Image),
    /// A sound from one of the banks
    Sound(Clip),
}
```

along with the glue between the two:

```rust
use enum_pair::{Paired, PairedData};

let asset: Asset = Image::load("grass.png").into();

// Which id a value belongs to, ignoring the id's own fields
assert!(AssetId::Texture(7).pairs_with(&asset));
assert!(!AssetId::Config.pairs_with(&asset));

// Typed access by payload type
let image: Option<&Image> = asset.get();
let image = Image::try_from(asset)?;

// Generic code written once and checked against every variant
fn save<T: serde::Serialize>(value: &T) -> String { /* ... */ }
let json = each_asset!(&asset, value => save(value));
```

In full, that's:

- `Paired<Asset>` for `AssetId`, with `pairs_with`
- `PairedData` for `Asset`, with `get`, `get_mut` and `into_variant`
- `From<Image> for Asset`, `TryFrom<Asset> for Image` and `Variant<Asset> for Image`, and the same for every other payload type that only one variant uses
- with `each = each_asset`, a crate-visible `each_asset!(data, pattern => body)` macro

## Several pairs

An id enum can pair with more than one set of types. Variants then name the pair each type is for, an `optional` pair can be left out, and a `types_only` pair skips the enum when you only need to get from an id to a type:

```rust
#[derive(EnumPair)]
#[pair(name = Asset, each_type = asset_type)]
#[pair(name = Decoder, types_only, optional, each_type = decoder_type)]
pub enum AssetId {
    #[pair(Asset = Image, Decoder = PngDecoder)]
    Texture(u32),
    #[pair(Asset = Clip, Decoder = OggDecoder)]
    Sound(u16),
    #[pair(Asset = Config)]
    Config,
}
```

`each_type` macros run generic code with `T` set to the type an id pairs with. That's the way back from an id to its data, with no `match` to keep in sync:

```rust
fn load(id: &AssetId, bytes: &[u8]) -> Option<Asset> {
    decoder_type!(id, T => T::decode(bytes).map(Asset::from)).flatten()
}

fn placeholder(id: &AssetId) -> Asset {
    asset_type!(id, T => Asset::from(T::default()))
}
```

`decoder_type!` gives `None` for `Config`, since `Decoder` is optional and `Config` has no entry for it.

The generated macros name types as written, so those have to be in scope where the macros are used.

## Comparison

Feature | enum_pair | [kinded] | [strum] | [enum-kinds] | [enum_dispatch]
--- | :---: | :---: | :---: | :---: | :---:
Id variants with their own fields | ✅ | ❌ | ❌ | ❌ | ❌
Generates the data enum | ✅ | ❌ | ❌ | ❌ | ❌
Generates a fieldless kind enum | ❌ | ✅ | ✅ | ✅ | ❌
Matches an id to its data | ✅ | ✅ | ✅ | ✅ | ❌
Derives on the generated enum | ✅ | ✅ | ✅ | ✅ | ❌
`From` for each payload type | ✅ | ❌ | ❌ | ❌ | ✅
Payload access by type | ✅ | ❌ | ❌ | ❌ | ✅
Generic code per variant | ✅ | ❌ | ❌ | ❌ | ✅
Generic code per id type | ✅ | ❌ | ❌ | ❌ | ❌
Several pairs per id | ✅ | ❌ | ❌ | ❌ | ❌
Lists every kind | ❌ | ✅ | ✅ | ❌ | ❌
`Display` and `FromStr` for kinds | ❌ | ✅ | ✅ | ❌ | ❌

[kinded]: https://crates.io/crates/kinded
[strum]: https://crates.io/crates/strum
[enum-kinds]: https://crates.io/crates/enum-kinds
[enum_dispatch]: https://crates.io/crates/enum_dispatch

The kind features are left to strum, since it works on both enums. Derive `EnumDiscriminants`, `EnumIter` or `Display` on the id enum, or pass `EnumIs` and `EnumTryAs` through `derive(...)` to get named accessors on the data enum, which also covers payload types that more than one variant shares.

## Options

Each `#[pair(...)]` on the id enum takes:

Option | Does
--- | ---
`name = Ident` | Names the data enum (required)
`vis = Visibility` | Sets its visibility, the id enum's by default
`derive(...)` | Derives traits on it
`attrs(...)` | Adds other attributes to it
`each = ident` | Generates `ident!(data, pattern => body)`
`each_type = ident` | Generates `ident!(id, T => body)`
`optional` | Lets variants leave the pair out
`types_only` | Skips the enum, leaving only `each_type`
`crate = path` | Points to a renamed `enum_pair`

With one pair, each variant takes `#[pair(Type)]`, optionally followed by `attrs(...)` for its generated variant. With several, entries name their pair, like `#[pair(Asset = Image, Decoder = PngDecoder)]`, or `Asset(Image, attrs(...))` to add attributes.

Generic and empty enums aren't supported.

## Development

Tasks run through [mise](https://mise.jdx.dev): `mise run check`, `mise run fix`, `mise run test`, `mise run correctness`, `mise run update` and `mise run upgrade`.
