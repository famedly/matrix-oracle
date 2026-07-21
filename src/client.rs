//! Resolution for the client-server API

pub mod error;

use std::collections::BTreeMap;

use reqwest::{StatusCode, Url};
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};

use self::error::{Error, FailError, UnexpectedStatus};
use crate::cache;

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
				.with(cache())
				.build(),
		}
	}

	/// Construct a new resolver with the given reqwest client.
	#[must_use]
	pub fn with(http: reqwest::Client) -> Self {
		Self { http: reqwest_middleware::ClientBuilder::new(http).with(cache()).build() }
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
		// b. otherwise, if the status code is not 200, FAIL_PROMPT
		if response.status() != StatusCode::OK {
			return Err(UnexpectedStatus(response.status()).into());
		}
		// c. parse the response as json
		let well_known = response.json::<ClientWellKnown>().await?;
		// d+e.i Extract base_url and parse it as a URL
		let url = Url::parse(&well_known.homeserver.base_url)?;
		// e.ii Validate versions endpoint: a non-200 response is a FAIL_ERROR
		let versions = self
			.http
			.get(url.join("_matrix/client/versions")?)
			.send()
			.await
			.map_err(FailError::Http)?;
		if versions.status() != StatusCode::OK {
			return Err(Error::Fail(UnexpectedStatus(versions.status()).into()));
		}
		versions.json::<Versions>().await.map_err(|e| FailError::Http(e.into()))?;

		// f. if present, validate identity server v2 endpoint
		if let Some(identity) = well_known.identity_server {
			let url = Url::parse(&identity.base_url)?;
			let result: Result<_, FailError> = async {
				let response = self.http.get(url.join("_matrix/identity/v2")?).send().await?;
				if response.status() != StatusCode::OK {
					return Err(UnexpectedStatus(response.status()).into());
				}
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
		Mock, MockServer, ResponseTemplate,
		matchers::{method, path},
	};

	use super::{Resolver, error::Error};

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

	/// Tests that a present identity server is validated at the current spec
	/// endpoint (`/_matrix/identity/v2`), not the removed v1 endpoint.
	#[tokio::test]
	async fn resolve_with_identity_server() -> Result<(), Box<dyn std::error::Error>> {
		let mock_server = MockServer::start().await;
		let port = mock_server.address().port();

		Mock::given(method("GET"))
			.and(path("/.well-known/matrix/client"))
			.respond_with(ResponseTemplate::new(200).set_body_raw(
				format!(
					r#"{{"m.homeserver":{{"base_url":"http://destination.test:{port}"}},"m.identity_server":{{"base_url":"http://destination.test:{port}"}}}}"#
				),
				"application/json",
			))
			.mount(&mock_server)
			.await;
		Mock::given(method("GET"))
			.and(path("/_matrix/client/versions"))
			.respond_with(
				ResponseTemplate::new(200)
					.set_body_raw(r#"{"versions":["v1.11"]}"#, "application/json"),
			)
			.mount(&mock_server)
			.await;
		// The identity server MUST be validated at the v2 endpoint.
		Mock::given(method("GET"))
			.and(path("/_matrix/identity/v2"))
			.respond_with(ResponseTemplate::new(200).set_body_raw("{}", "application/json"))
			.expect(1)
			.mount(&mock_server)
			.await;

		let http = reqwest::Client::builder()
			.resolve("example.test", *mock_server.address())
			.resolve("destination.test", *mock_server.address())
			.build()?;
		let resolver = Resolver::with(http);
		let url = resolver.resolve(&format!("example.test:{port}")).await?;
		assert_eq!(url.to_string(), format!("http://destination.test:{port}/"));
		Ok(())
	}

	/// Per spec: a non-404, non-200 response to the well-known request must
	/// `FAIL_PROMPT`, even if the body happens to parse as valid JSON.
	#[tokio::test]
	async fn well_known_non_200_fails_prompt() -> Result<(), Box<dyn std::error::Error>> {
		let mock_server = MockServer::start().await;
		let port = mock_server.address().port();

		Mock::given(method("GET"))
			.and(path("/.well-known/matrix/client"))
			.respond_with(ResponseTemplate::new(500).set_body_raw(
				format!(r#"{{"m.homeserver":{{"base_url":"http://destination.test:{port}"}}}}"#),
				"application/json",
			))
			.expect(1)
			.mount(&mock_server)
			.await;

		let http = reqwest::Client::builder()
			.resolve("example.test", *mock_server.address())
			.resolve("destination.test", *mock_server.address())
			.build()?;
		let resolver = Resolver::with(http);
		let result = resolver.resolve(&format!("example.test:{port}")).await;

		assert!(matches!(result, Err(Error::Prompt(_))), "expected FAIL_PROMPT, got {result:?}");
		Ok(())
	}

	/// Per spec: a failed `/_matrix/client/versions` validation must
	/// `FAIL_ERROR`, even if the response body happens to parse as `Versions`.
	#[tokio::test]
	async fn versions_non_200_fails_error() -> Result<(), Box<dyn std::error::Error>> {
		let mock_server = MockServer::start().await;
		let port = mock_server.address().port();

		Mock::given(method("GET"))
			.and(path("/.well-known/matrix/client"))
			.respond_with(ResponseTemplate::new(200).set_body_raw(
				format!(r#"{{"m.homeserver":{{"base_url":"http://destination.test:{port}"}}}}"#),
				"application/json",
			))
			.mount(&mock_server)
			.await;
		Mock::given(method("GET"))
			.and(path("/_matrix/client/versions"))
			.respond_with(
				ResponseTemplate::new(500)
					.set_body_raw(r#"{"versions":["v1.11"]}"#, "application/json"),
			)
			.expect(1)
			.mount(&mock_server)
			.await;

		let http = reqwest::Client::builder()
			.resolve("example.test", *mock_server.address())
			.resolve("destination.test", *mock_server.address())
			.build()?;
		let resolver = Resolver::with(http);
		let result = resolver.resolve(&format!("example.test:{port}")).await;

		assert!(matches!(result, Err(Error::Fail(_))), "expected FAIL_ERROR, got {result:?}");
		Ok(())
	}

	/// Per spec: only a `200` well-known response is accepted. A
	/// 2xx-but-not-200 status (e.g. `201`) must `FAIL_PROMPT`, even with a
	/// parseable body.
	#[tokio::test]
	async fn well_known_2xx_non_200_fails_prompt() -> Result<(), Box<dyn std::error::Error>> {
		let mock_server = MockServer::start().await;
		let port = mock_server.address().port();

		Mock::given(method("GET"))
			.and(path("/.well-known/matrix/client"))
			.respond_with(ResponseTemplate::new(201).set_body_raw(
				format!(r#"{{"m.homeserver":{{"base_url":"http://destination.test:{port}"}}}}"#),
				"application/json",
			))
			.expect(1)
			.mount(&mock_server)
			.await;
		Mock::given(method("GET"))
			.and(path("/_matrix/client/versions"))
			.respond_with(
				ResponseTemplate::new(200)
					.set_body_raw(r#"{"versions":["v1.11"]}"#, "application/json"),
			)
			.mount(&mock_server)
			.await;

		let http = reqwest::Client::builder()
			.resolve("example.test", *mock_server.address())
			.resolve("destination.test", *mock_server.address())
			.build()?;
		let resolver = Resolver::with(http);
		let result = resolver.resolve(&format!("example.test:{port}")).await;

		assert!(matches!(result, Err(Error::Prompt(_))), "expected FAIL_PROMPT, got {result:?}");
		Ok(())
	}

	/// Per spec: homeserver validation requires a `200` from
	/// `/_matrix/client/versions`. A 2xx-but-not-200 status must `FAIL_ERROR`,
	/// even with a parseable body.
	#[tokio::test]
	async fn versions_2xx_non_200_fails_error() -> Result<(), Box<dyn std::error::Error>> {
		let mock_server = MockServer::start().await;
		let port = mock_server.address().port();

		Mock::given(method("GET"))
			.and(path("/.well-known/matrix/client"))
			.respond_with(ResponseTemplate::new(200).set_body_raw(
				format!(r#"{{"m.homeserver":{{"base_url":"http://destination.test:{port}"}}}}"#),
				"application/json",
			))
			.mount(&mock_server)
			.await;
		Mock::given(method("GET"))
			.and(path("/_matrix/client/versions"))
			.respond_with(
				ResponseTemplate::new(201)
					.set_body_raw(r#"{"versions":["v1.11"]}"#, "application/json"),
			)
			.expect(1)
			.mount(&mock_server)
			.await;

		let http = reqwest::Client::builder()
			.resolve("example.test", *mock_server.address())
			.resolve("destination.test", *mock_server.address())
			.build()?;
		let resolver = Resolver::with(http);
		let result = resolver.resolve(&format!("example.test:{port}")).await;

		assert!(matches!(result, Err(Error::Fail(_))), "expected FAIL_ERROR, got {result:?}");
		Ok(())
	}

	/// Per spec: identity-server validation requires a `200` from
	/// `/_matrix/identity/v2`. A 2xx-but-not-200 status must `FAIL_ERROR`.
	#[tokio::test]
	async fn identity_2xx_non_200_fails_error() -> Result<(), Box<dyn std::error::Error>> {
		let mock_server = MockServer::start().await;
		let port = mock_server.address().port();

		Mock::given(method("GET"))
			.and(path("/.well-known/matrix/client"))
			.respond_with(ResponseTemplate::new(200).set_body_raw(
				format!(
					r#"{{"m.homeserver":{{"base_url":"http://destination.test:{port}"}},"m.identity_server":{{"base_url":"http://destination.test:{port}"}}}}"#
				),
				"application/json",
			))
			.mount(&mock_server)
			.await;
		Mock::given(method("GET"))
			.and(path("/_matrix/client/versions"))
			.respond_with(
				ResponseTemplate::new(200)
					.set_body_raw(r#"{"versions":["v1.11"]}"#, "application/json"),
			)
			.mount(&mock_server)
			.await;
		Mock::given(method("GET"))
			.and(path("/_matrix/identity/v2"))
			.respond_with(ResponseTemplate::new(204))
			.expect(1)
			.mount(&mock_server)
			.await;

		let http = reqwest::Client::builder()
			.resolve("example.test", *mock_server.address())
			.resolve("destination.test", *mock_server.address())
			.build()?;
		let resolver = Resolver::with(http);
		let result = resolver.resolve(&format!("example.test:{port}")).await;

		assert!(matches!(result, Err(Error::Fail(_))), "expected FAIL_ERROR, got {result:?}");
		Ok(())
	}
}
