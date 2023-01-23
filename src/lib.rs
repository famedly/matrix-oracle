//! Matrix-oracle is a crate for performing lookups of .well-known information
//! for the matrix protocol.
//!
//! # Features
#![cfg_attr(doc, doc = document_features::document_features!())]
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

use http_cache_reqwest::{Cache, CacheMode, CacheOptions, HttpCache, MokaManager};

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

/// Returns a HTTP caching middleware with appropriate settings for
/// matrix-oracle's use-case.
pub(crate) fn cache() -> Cache<MokaManager> {
	Cache(HttpCache {
		mode: CacheMode::Default,
		manager: MokaManager::default(),
		options: Some(CacheOptions { shared: false, ..CacheOptions::default() }),
	})
}
