//! Hash map selection for the renderer's texture table.
//!
//! The default `rustc-hash` feature uses [`rustc_hash::FxBuildHasher`], which is fast for the
//! integer texture keys. Disable default features to fall back to the standard library's randomly
//! seeded hasher.

/// The hash map type used by the renderer.
pub(crate) type HashMap<K, V> = std::collections::HashMap<K, V, DefaultHashBuilder>;

#[cfg(feature = "rustc-hash")]
type DefaultHashBuilder = rustc_hash::FxBuildHasher;

#[cfg(not(feature = "rustc-hash"))]
type DefaultHashBuilder = std::hash::RandomState;
