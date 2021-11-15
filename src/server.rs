//! Resolution for the server-server API

use std::net::{IpAddr, SocketAddr};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};
use trust_dns_resolver::{
	error::{ResolveError, ResolveErrorKind},
	TokioAsyncResolver,
};

pub mod error;

/// well-known information about the delegated server for server-server
/// communication.
///
/// See [the specification] for more information.
///
/// [the specification]: https://matrix.org/docs/spec/server_server/latest#get-well-known-matrix-server
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerWellKnown {
	/// The server name to delegate server-server communciations to, with
	/// optional port
	#[serde(rename = "m.server")]
	pub server: String,
}

/// Client for server-server well-known lookups.
#[derive(Debug, Clone)]
pub struct Resolver {
	/// HTTP client.
	http: reqwest::Client,
	/// DNS resolver.
	resolver: TokioAsyncResolver,
}

/// Resolved server name
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Server {
	/// IP address with implicit default port (8448)
	Ip(IpAddr),
	/// IP address and explicit port
	Socket(SocketAddr),
	/// Host string with implicit default port (8448)
	Host(String),
	/// Host string with explicit port.
	HostPort(String),
	/// Address from srv record, hostname from server name.
	Srv(String, String),
}

impl Server {
	/// The value to use for the `Host` HTTP header.
	pub fn host_header(&self) -> String {
		match self {
			Server::Ip(addr) => addr.to_string(),
			Server::Socket(addr) => addr.to_string(),
			Server::Host(host) => host.to_owned(),
			Server::HostPort(host) => host.to_owned(),
			Server::Srv(_, host) => host.to_string(),
		}
	}

	/// The address to connect to.
	pub fn address(&self) -> String {
		match self {
			Server::Ip(addr) => format!("{}:8448", addr),
			Server::Socket(addr) => addr.to_string(),
			Server::Host(host) => format!("{}:8448", host),
			Server::HostPort(host) => host.clone(),
			Server::Srv(host, _) => host.clone(),
		}
	}
}

impl Resolver {
	/// Constructs a new client.
	pub fn new() -> Result<Self, ResolveError> {
		Ok(Self {
			http: reqwest::Client::new(),
			resolver: TokioAsyncResolver::tokio_from_system_conf()?,
		})
	}

	/// Constructs a new client with the given HTTP client and DNS resolver
	/// instances.
	pub fn with(http: reqwest::Client, resolver: TokioAsyncResolver) -> Self {
		Self { http, resolver }
	}

	/// Resolve the given server name
	#[instrument(skip(self), err)]
	pub async fn resolve(&self, name: &str) -> error::Result<Server> {
		// 1. The host is an ip literal
		debug!("Parsing socket literal");
		if let Ok(addr) = name.parse::<SocketAddr>() {
			info!("The server name is a socket literal");
			return Ok(Server::Socket(addr));
		}
		debug!("Parsing IP literal");
		if let Ok(addr) = name.parse::<IpAddr>() {
			info!("The server name is an IP literal");
			return Ok(Server::Ip(addr));
		}
		// 2. The host is not an ip literal, but includes a port
		debug!("Parsing host with port");
		if split_port(name).is_some() {
			info!("The servername is a host with port");
			return Ok(Server::HostPort(name.to_owned()));
		}
		// 3. Query the .well-known endpoint
		debug!("Querying well known");
		if let Some(well_known) = self.well_known(name).await? {
			debug!("Well-known received: {:?}", &well_known);
			// 3.1 delegated_hostname is an ip literal
			debug!("Parsing delegated socket literal");
			if let Ok(addr) = well_known.server.parse::<SocketAddr>() {
				info!("The server name is a delegated IP literal");
				return Ok(Server::Socket(addr));
			}
			debug!("Parsing delegated IP literal");
			if let Ok(addr) = well_known.server.parse::<IpAddr>() {
				info!("The server name is a delegated socket literal");
				return Ok(Server::Ip(addr));
			}
			// 3.2 delegated_hostname includes a port
			debug!("Parsing delegated hostname with port");
			if split_port(&well_known.server).is_some() {
				info!("The server name is a delegated hostname with port");
				return Ok(Server::HostPort(well_known.server));
			}
			// 3.3 Look up SRV record
			debug!("Looking up SRV record for delegated hostname");
			if let Some(name) = self.srv_lookup(&well_known.server).await {
				info!("The server name is a delegated SRV record");
				return Ok(Server::Srv(name, well_known.server));
			}
			// 3.4 Use hostname in .well-known
			debug!("Using delegated hostname directly");
			return Ok(Server::Host(well_known.server));
		}
		// 4. The .well-known lookup failed, query SRV
		debug!("Looking up SRV record for hostname");
		if let Some(srv) = self.srv_lookup(name).await {
			info!("The server name is an SRV record");
			return Ok(Server::Srv(srv, name.to_owned()));
		}
		// 5. No SRV record found, use hostname
		debug!("Using provided hostname directly");
		Ok(Server::Host(name.to_owned()))
	}

	/// Query the .well-known information for a host.
	#[cfg_attr(test, allow(unused_variables))]
	#[instrument(skip(self, name), err)]
	async fn well_known(&self, name: &str) -> error::Result<Option<ServerWellKnown>> {
		#[cfg(not(test))]
		let response = self.http.get(format!("https://{}/.well-known/matrix/server", name)).send().await;

		#[cfg(test)]
		let response = self
			.http
			.get(format!("http://{}/.well-known/matrix/server", mockito::server_address()))
			.send()
			.await;

		// Only return Err on connection failure, skip to next step for other errors.
		let response = match response {
			Ok(response) => response,
			Err(e) if e.is_connect() => return Err(e.into()),
			Err(_) => return Ok(None),
		};
		let well_known = response.json::<ServerWellKnown>().await.ok();
		Ok(well_known)
	}

	/// Query the matrix SRV DNS record for a hostname
	#[instrument(skip(self, name))]
	async fn srv_lookup(&self, name: &str) -> Option<String> {
		let srv = self.resolver.srv_lookup(format!("_matrix._tcp.{}", name)).await.ok()?;
		// Get a record with the lowest priority value
		match srv.iter().min_by_key(|srv| srv.priority()) {
			Some(srv) => {
				let target = srv.target().to_ascii();
				let host = target.trim_end_matches('.');
				Some(format!("{}:{}", host, srv.port()))
			}
			None => None,
		}
	}

	/// Get the [`SocketAddr`] of an address
	pub async fn socket(&self, server: &Server) -> Result<SocketAddr, ResolveError> {
		let (host, port) = match *server {
			Server::Ip(ip) => return Ok(SocketAddr::new(ip, 8448)),
			Server::Socket(socket) => return Ok(socket),
			Server::Host(ref host) => (host.as_str(), 8448),
			#[allow(clippy::expect_used)]
			Server::HostPort(ref host) => split_port(host).expect("HostPort was constructed with port"),
			#[allow(clippy::expect_used)]
			Server::Srv(ref addr, _) => split_port(addr).expect("The SRV record includes the port"),
		};
		let record = self.resolver.lookup_ip(host).await?;
		// We naively get the first IP.
		let socket = SocketAddr::new(
			record.iter().next().ok_or(ResolveErrorKind::Message("No records"))?,
			port,
		);
		Ok(socket)
	}
}

/// Get the port at the end of a host string if there is one.
fn split_port(host: &str) -> Option<(&str, u16)> {
	match &host.split(':').collect::<Vec<_>>()[..] {
		[host, port] => match port.parse() {
			Ok(port) => Some((host, port)),
			Err(_) => None,
		},
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use std::net::{IpAddr, SocketAddr};

	use mockito::mock;
	use trust_dns_resolver::TokioAsyncResolver;

	use super::{Resolver, Server};

	/// Validates correct parsing of IP literals and server name with port
	#[tokio::test]
	async fn literals() -> Result<(), Box<dyn std::error::Error>> {
		let resolver = Resolver::new()?;
		assert_eq!(
			resolver.resolve("127.0.0.1").await?,
			Server::Ip(IpAddr::from([127, 0, 0, 1])),
			"1. IP literal"
		);
		assert_eq!(
			resolver.resolve("127.0.0.1:4884").await?,
			Server::Socket(SocketAddr::new(IpAddr::from([127, 0, 0, 1]), 4884)),
			"1. Socket literal"
		);
		assert_eq!(
			resolver.resolve("example.test:1234").await?,
			Server::HostPort(String::from("example.test:1234")),
			"2. Host with port"
		);
		Ok(())
	}

	/// Validates correct handing of the .well-known http endpoint.
	#[tokio::test]
	async fn http() -> Result<(), Box<dyn std::error::Error>> {
		let client = reqwest::Client::builder()
			.resolve("example.test", mockito::server_address())
			.resolve("destination.test", mockito::server_address())
			.build()?;
		let resolver = Resolver::with(client, TokioAsyncResolver::tokio_from_system_conf()?);

		let addr = mockito::server_address();

		let _ip = mock("GET", "/.well-known/matrix/server")
			.with_body(format!(r#"{{"m.server": "{}"}}"#, addr.ip()))
			.create();
		assert_eq!(
			resolver.resolve("example.test").await?,
			Server::Ip(addr.ip()),
			"3.1 delegated_hostname is an IP literal"
		);

		let _sock = mock("GET", "/.well-known/matrix/server")
			.with_body(format!(r#"{{"m.server": "{}"}}"#, addr))
			.create();
		assert_eq!(
			resolver.resolve("example.test").await?,
			Server::Socket(mockito::server_address()),
			"3.1 delegated_hostname is a socket literal"
		);

		let _port = mock("GET", "/.well-known/matrix/server")
			.with_body(format!(r#"{{"m.server": "destination.test:{}"}}"#, addr.port()))
			.create();
		assert_eq!(
			resolver.resolve("example.test").await?,
			Server::HostPort(format!("destination.test:{}", addr.port())),
			"3.2 delegated_hostname includes a port"
		);
		Ok(())
	}
}
