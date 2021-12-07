//! Matrix-oracle is a crate for performing lookups of .well-known information
//! for the matrix protocol.
//!
//! # Features
//! * `client` - Enable client-server .well-known lookups (enabled by default)
//! * `server` - Enable server-server .well-known lookups (enabled by default)
//! * `native-tls` - Use openssl via native-tls as the TLS implementation
//!   (enabled by default)
//! * `rustls` - Use rustls as the TLS implementation

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
