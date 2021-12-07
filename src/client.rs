//! Resolution for the client-server API

pub mod error;

use std::collections::BTreeMap;

use reqwest::{StatusCode, Url};
use reqwest_cache::CacheMiddleware;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};

use self::error::{Error, FailError};
use crate::cache_options;

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

/// Resolver for well-known lookups for the client-server API.
#[derive(Clone, Debug)]
pub struct Resolver {
	http: ClientWithMiddleware,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Versions {
	pub versions: Vec<String>,
	#[serde(default)]
	pub unstable_features: BTreeMap<String, bool>,
}

impl Resolver {
	/// Construct a new resolver.
	pub fn new() -> Self {
		Self {
			http: reqwest_middleware::ClientBuilder::new(reqwest::Client::new())
				.with(CacheMiddleware::with_options(cache_options()))
				.build(),
		}
	}

	/// Construct a new resolver with the given reqwest client.
	pub fn with(http: reqwest::Client) -> Self {
		Self {
			http: reqwest_middleware::ClientBuilder::new(http)
				.with(CacheMiddleware::with_options(cache_options()))
				.build(),
		}
	}

	/// Get the base URL for the client-server API with the given name.
	pub async fn resolve(&self, name: &str) -> Result<Url, Error> {
		#[cfg(not(test))]
		let url = Url::parse(&format!("https://{}", name))?;
		#[cfg(test)]
		let url = Url::parse(&format!("http://{}:{}", name, mockito::server_address().port()))?;

		// 3. make a GET request to the well-known endpoint
		let response = self.http.get(url.join(".well-known/matrix/client")?).send().await?;
		// a. if the returned status code is 404, then IGNORE
		if response.status() == StatusCode::NOT_FOUND {
			return Ok(url);
		};
		// c. parse the response as json
		let well_known = response.json::<ClientWellKnown>().await?;
		// d+e.i Extract base_url and parse it as a URL
		let url = Url::parse(&well_known.homeserver.base_url)?;
		// e.ii Validate versions endpoint
		self.http
			.get(url.join("_matrix/client/versions")?)
			.send()
			.await
			.map_err(FailError::Http)?
			.json::<Versions>()
			.await
			.map_err(|e| FailError::Http(e.into()))?;

		// f. if present, validate identity server endpoint
		if let Some(identity) = well_known.identity_server {
			let url = Url::parse(&identity.base_url)?;
			let result: Result<_, FailError> = async {
				self.http
					.get(url.join("_matrix/identity/api/v1")?)
					.send()
					.await?
					.error_for_status()?;
				Ok(())
			}
			.await;
			result?;
		}

		Ok(url)
	}
}

impl Default for Resolver {
	fn default() -> Self {
		Self { http: ClientWithMiddleware::from(reqwest::Client::new()) }
	}
}

#[cfg(test)]
mod tests {
	use mockito::mock;

	use super::Resolver;

	/// Tests that a 404 response is correctly handled
	#[tokio::test]
	async fn not_found() -> Result<(), Box<dyn std::error::Error>> {
		let mock = mock("GET", "/.well-known/matrix/client").with_status(404).create();
		let http = reqwest::Client::builder()
			.resolve("example.test", mockito::server_address())
			.build()?;
		let resolver = Resolver::with(http);
		let url = resolver.resolve("example.test").await?;

		assert_eq!(
			format!("http://example.test:{}/", mockito::server_address().port()),
			url.to_string()
		);
		mock.assert();
		Ok(())
	}

	#[tokio::test]
	async fn resolve() -> Result<(), Box<dyn std::error::Error>> {
		let port = mockito::server_address().port();
		let well_known = mock("GET", "/.well-known/matrix/client")
			.with_body(format!(
				r#"{{"m.homeserver": {{"base_url": "http://destination.test:{}"}} }}"#,
				port
			))
			.create();
		let versions = mock("GET", "/_matrix/client/versions")
			.with_body(r#"{"versions":["r0.0.1"]}"#)
			.create();

		let http = reqwest::Client::builder()
			.resolve("example.test", mockito::server_address())
			.resolve("destination.test", mockito::server_address())
			.build()?;
		let resolver = Resolver::with(http);

		let url = resolver.resolve("example.test").await?;

		assert_eq!(url.to_string(), format!("http://destination.test:{}/", port));
		well_known.assert();
		versions.assert();
		Ok(())
	}
}
