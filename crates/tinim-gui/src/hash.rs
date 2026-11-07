//! Hash map selection for internal caches.
//!
//! The default `rustc-hash` feature uses [`rustc_hash::FxBuildHasher`], which is fast for the
//! small string keys used by the text cache. Disable default features to fall back to the
//! standard library's randomly seeded hasher.

/// The hash map type used by internal caches.
pub(crate) type HashMap<K, V> = std::collections::HashMap<K, V, DefaultHashBuilder>;

#[cfg(feature = "rustc-hash")]
type DefaultHashBuilder = rustc_hash::FxBuildHasher;

#[cfg(not(feature = "rustc-hash"))]
type DefaultHashBuilder = std::hash::RandomState;
