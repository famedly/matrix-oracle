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
	/// The HTTP client used to send and receive requests. Should transparently
	/// handle HTTP caching.
	http: ClientWithMiddleware,
}

/// Represents the set of matrix versions a server support. Used exclusively for
/// validating the contents of a response
#[allow(dead_code)]
#[derive(Deserialize)]
struct Versions {
	/// List of matrix spec versions the server supports.
	pub versions: Vec<String>,
	/// Set of unstable matrix extensions which the server supports
	#[serde(default)]
	pub unstable_features: BTreeMap<String, bool>,
}

impl Resolver {
	/// Construct a new resolver.
	#[must_use]
	pub fn new() -> Self {
		Self {
			http: reqwest_middleware::ClientBuilder::new(reqwest::Client::new())
				.with(CacheMiddleware::with_options(cache_options()))
				.build(),
		}
	}

	/// Construct a new resolver with the given reqwest client.
	#[must_use]
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
		let url = Url::parse(&format!("http://{}", name))?;

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
	use wiremock::{
		matchers::{method, path},
		Mock, MockServer, ResponseTemplate,
	};

	use super::Resolver;

	/// Tests that a 404 response is correctly handled
	#[tokio::test]
	async fn not_found() -> Result<(), Box<dyn std::error::Error>> {
		let mock_server = MockServer::start().await;

		Mock::given(method("GET"))
			.and(path("/.well-known/matrix/client"))
			.respond_with(ResponseTemplate::new(404))
			.expect(1)
			.mount(&mock_server)
			.await;

		let http =
			reqwest::Client::builder().resolve("example.test", *mock_server.address()).build()?;
		let resolver = Resolver::with(http);
		let url =
			resolver.resolve(&format!("example.test:{}", mock_server.address().port())).await?;

		assert_eq!(
			format!("http://example.test:{}/", mock_server.address().port()),
			url.to_string()
		);
		Ok(())
	}

	#[tokio::test]
	async fn resolve() -> Result<(), Box<dyn std::error::Error>> {
		let mock_server = MockServer::start().await;

		let port = mock_server.address().port();

		Mock::given(method("GET"))
			.and(path("/.well-known/matrix/client"))
			.respond_with(ResponseTemplate::new(200).set_body_raw(
				format!(
					r#"{{"m.homeserver": {{"base_url": "http://destination.test:{}"}} }}"#,
					port
				),
				"application/json",
			))
			.expect(1)
			.mount(&mock_server)
			.await;

		Mock::given(method("GET"))
			.and(path("/_matrix/client/versions"))
			.respond_with(
				ResponseTemplate::new(200)
					.set_body_raw(r#"{"versions":["r0.0.1"]}"#, "application/json"),
			)
			.expect(1)
			.mount(&mock_server)
			.await;

		let http = reqwest::Client::builder()
			.resolve("example.test", *mock_server.address())
			.resolve("destination.test", *mock_server.address())
			.build()?;
		let resolver = Resolver::with(http);

		let url =
			resolver.resolve(&format!("example.test:{}", mock_server.address().port())).await?;

		assert_eq!(url.to_string(), format!("http://destination.test:{}/", port));
		Ok(())
	}
}
