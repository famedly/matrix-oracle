//! Errors that can occur during client-server lookup

/// Errors that can occur during lookup. Refer to [the specification] to see how
/// the two variants should be handled.
///
/// [the spec]: https://matrix.org/docs/spec/client_server/latest#well-known-uri
#[derive(Debug)]
pub enum Error {
	/// Corresponds to the `FAIL_PROMPT` code in the spec.
	Prompt(reqwest_middleware::Error),
	/// Corresponds to the `FAIL_ERROR` code in the spec.
	Fail(FailError),
}

impl std::error::Error for Error {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match *self {
			Self::Prompt(ref e) => Some(e),
			Self::Fail(ref e) => Some(e),
		}
	}
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Prompt(e) => write!(f, "{}", e),
			Self::Fail(e) => write!(f, "{}", e),
		}
	}
}

impl From<reqwest::Error> for Error {
	fn from(e: reqwest::Error) -> Self {
		Error::Prompt(e.into())
	}
}

impl From<reqwest_middleware::Error> for Error {
	fn from(e: reqwest_middleware::Error) -> Self {
		Error::Prompt(e)
	}
}

impl From<url::ParseError> for Error {
	fn from(e: url::ParseError) -> Self {
		Error::Fail(FailError::Url(e))
	}
}

impl From<FailError> for Error {
	fn from(e: FailError) -> Self {
		Error::Fail(e)
	}
}

/// Corresponds to the `FAIL_PROMPT` code in the spec.
#[derive(Debug)]
pub enum FailError {
	/// URL parsing error
	Url(url::ParseError),
	/// HTTP error
	Http(reqwest_middleware::Error),
}

impl std::error::Error for FailError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match *self {
			Self::Http(ref e) => Some(e),
			Self::Url(ref e) => Some(e),
		}
	}
}

impl std::fmt::Display for FailError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Http(e) => write!(f, "{}", e),
			Self::Url(e) => write!(f, "{}", e),
		}
	}
}

impl From<reqwest::Error> for FailError {
	fn from(e: reqwest::Error) -> Self {
		FailError::Http(e.into())
	}
}

impl From<reqwest_middleware::Error> for FailError {
	fn from(e: reqwest_middleware::Error) -> Self {
		FailError::Http(e)
	}
}

impl From<url::ParseError> for FailError {
	fn from(e: url::ParseError) -> Self {
		FailError::Url(e)
	}
}
