//! Resolution for the client-server API

use serde::{Deserialize, Serialize};

/// well-known information for the client-server API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientWellKnown {
	/// Information about the homeserver to connect to.
	#[serde(rename = "m.homeserver")]
	pub homeserver: HomeserverInfo,

	/// Information about the identity server to connect to.
	#[serde(rename = "m.identity_server")]
	pub identity_server: Option<IdentityServerInfo>,
}

/// Information about the homeserver to connect to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeserverInfo {
	/// The base url to use for client-server API endpoints.
	base_url: String,
}

/// Information about the identity server to connect to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityServerInfo {
	/// The base url to use for identity server API endpoints.
	base_url: String,
}
