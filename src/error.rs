//! Errors that can happen while performing lookup.

/// The result of attempting to perform well-known lookup.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can happen when attempting to perform well-known lookup.
#[derive(Debug)]
pub enum Error {
	/// An error happened while fetching an HTTP request.
	Http(reqwest::Error),
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Http(http) => write!(f, "{}", http),
		}
	}
}

impl std::error::Error for Error {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match *self {
			Self::Http(ref err) => Some(err),
		}
	}
}

impl From<reqwest::Error> for Error {
	fn from(err: reqwest::Error) -> Self {
		Self::Http(err)
	}
}
