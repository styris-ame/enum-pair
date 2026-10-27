//! Pairs an id enum with one or more data enums, variant by variant.
//!
//! Crates like `strum` (`EnumDiscriminants`) or `kinded` go from a data enum to a fieldless kind enum.
//! That breaks down when the id itself has to carry something, like an index or a lookup key. Here
//! the id enum is the source: its variants keep whatever fields they need, and each one names the type
//! of data it stands for.
//!
//! ```
//! use enum_pair::{EnumPair, Paired, PairedData};
//!
//! #[derive(Debug)]
//! pub struct Config { volume: u8 }
//! #[derive(Debug)]
//! pub struct Image { width: u32, height: u32 }
//! #[derive(Debug)]
//! pub struct Clip { seconds: f32 }
//!
//! #[derive(EnumPair)]
//! #[pair(name = Asset, derive(Debug), each = each_asset)]
//! pub enum AssetId {
//!     #[pair(Config)]
//!     Config,
//!     #[pair(Image)]
//!     Texture(u32),
//!     #[pair(Clip)]
//!     Sound { bank: u8, index: u16 },
//! }
//!
//! let mut asset: Asset = Image { width: 64, height: 32 }.into();
//! assert!(AssetId::Texture(7).pairs_with(&asset));
//! assert!(!AssetId::Sound { bank: 0, index: 3 }.pairs_with(&asset));
//!
//! assert_eq!(asset.get::<Image>().map(|image| image.width), Some(64));
//! assert!(asset.get::<Clip>().is_none());
//! if let Some(image) = asset.get_mut::<Image>() {
//!     image.height = 64;
//! }
//!
//! assert_eq!(each_asset!(&asset, value => format!("{value:?}")), "Image { width: 64, height: 64 }");
//!
//! let asset = Config::try_from(asset).unwrap_err();
//! assert_eq!(Image::try_from(asset).unwrap().height, 64);
//! ```
//!
//! # Several pairs
//!
//! An id enum can pair with more than one set of types, each from its own `#[pair(...)]`. Variants then
//! name the pair each type is for. An `optional` pair lets variants leave it out, and a `types_only`
//! pair skips the enum when all you need is to get from an id to a type with `each_type`.
//!
//! ```
//! use enum_pair::EnumPair;
//!
//! # #[derive(Default)] pub struct Image;
//! # #[derive(Default)] pub struct Clip;
//! pub struct PngHeader([u8; 8]);
//!
//! #[derive(EnumPair)]
//! #[pair(name = Asset, each_type = asset_type)]
//! #[pair(name = Header, types_only, optional, each_type = header_type)]
//! pub enum AssetId {
//!     #[pair(Asset = Image, Header = PngHeader)]
//!     Texture(u32),
//!     #[pair(Asset = Clip)]
//!     Sound(u16),
//! }
//!
//! fn placeholder(id: &AssetId) -> Asset {
//!     asset_type!(id, T => Asset::from(T::default()))
//! }
//!
//! assert!(matches!(placeholder(&AssetId::Sound(2)), Asset::Sound(_)));
//! assert_eq!(header_type!(&AssetId::Texture(1), T => size_of::<T>()), Some(8));
//! assert_eq!(header_type!(&AssetId::Sound(2), T => size_of::<T>()), None);
//! ```
//!
//! # Enum options
//!
//! Each `#[pair(...)]` on the id enum takes:
//!
//! - `name = Ident`, required: the data enum to generate.
//! - `vis = Visibility`: its visibility, the id enum's by default.
//! - `derive(...)`: derives for it.
//! - `attrs(...)`: any other attributes for it, written without `#[]`, like `attrs(non_exhaustive, doc = "...")`.
//! - `each = ident`: also generate `ident!(data, pattern => body)`, which runs `body` for whichever
//!   variant `data` holds with its value bound to `pattern`.
//! - `each_type = ident`: also generate `ident!(id, T => body)`, which runs `body` with `T` as the type
//!   the id's variant pairs with. It gives an `Option` for an `optional` pair, `None` for variants
//!   without one.
//! - `optional`: lets variants leave this pair out. They get no variant in the data enum.
//! - `types_only`: skips the enum and everything built on it, leaving only `each_type`.
//! - `crate = path`: where this crate lives, if it was renamed or re-exported. Given once for all pairs.
//!
//! The macros are usable anywhere in the crate. Their bodies are written once but typechecked per
//! variant, so they can call generic code that a trait object couldn't. They name the enums and types
//! as written, so those have to be in scope where the macros are used.
//!
//! # Variant options
//!
//! With one pair, each variant takes `#[pair(Type)]`, optionally followed by `attrs(...)` for its
//! generated variant. With several, entries name their pair, like `#[pair(Asset = Image, Header = PngHeader)]`,
//! or `Asset(Image, attrs(...))` to add attributes. Every pair that isn't `optional` needs an entry.
//!
//! # Generated items
//!
//! For each pair that isn't `types_only`:
//!
//! - The data enum, with a single-field tuple variant per paired id variant.
//! - [`Paired`] for the id enum and [`PairedData`] for the data enum.
//! - `From<Type>` for the data enum, plus `TryFrom` back and [`Variant`] for each `Type`, except for types used by more than
//!   one variant, since a value of those can't tell which variant it belongs in. Types are compared as
//!   written, so two spellings of the same type still count as distinct and will clash.
//!
//! Generic and empty enums aren't supported.

#![no_std]

pub use enum_pair_derive::EnumPair;

/// An id enum whose variants stand for variants of the data enum `D`
pub trait Paired<D: PairedData<Id = Self>>: Sized {
    /// Whether `data` is in the variant this id stands for, ignoring the id's own fields
    fn pairs_with(&self, data: &D) -> bool;
}

/// A data enum generated from a [`Paired`] id enum
pub trait PairedData: Sized {
    type Id: Paired<Self>;

    fn get<T: Variant<Self>>(&self) -> Option<&T> {
        T::from_ref(self)
    }

    fn get_mut<T: Variant<Self>>(&mut self) -> Option<&mut T> {
        T::from_mut(self)
    }

    /// Takes the value out if it's a `T`
    ///
    /// # Errors
    ///
    /// Gives the data back if it holds some other variant.
    fn into_variant<T: Variant<Self>>(self) -> Result<T, Self> {
        T::try_from(self)
    }
}

/// A type that exactly one variant of the data enum `E` holds
pub trait Variant<E>: Into<E> + TryFrom<E, Error = E> {
    fn from_ref(data: &E) -> Option<&Self>;

    fn from_mut(data: &mut E) -> Option<&mut Self>;
}
