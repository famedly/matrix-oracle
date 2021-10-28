//! Matrix-oracle is a crate for performing lookups of .well-known information
//! for the matrix protocol.

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

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;
