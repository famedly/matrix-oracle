//! Matrix-oracle is a crate for performing lookups of .well-known information
//! for the matrix protocol.
//!
//! # Features
#![doc = document_features::document_features!()]
#![deny(
	trivial_casts,
	trivial_numeric_casts,
	unused_extern_crates,
	unused_import_braces,
	unused_qualifications
)]
#![warn(
	missing_debug_implementations,
	missing_docs,
	dead_code,
	clippy::unwrap_used,
	clippy::expect_used
)]

use reqwest_cache::CacheOptions;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

// There's no const default constructor for CacheOptions, so we have to make a
// function instead of a constant.
pub(crate) fn cache_options() -> CacheOptions {
	CacheOptions { shared: false, ..CacheOptions::default() }
}
