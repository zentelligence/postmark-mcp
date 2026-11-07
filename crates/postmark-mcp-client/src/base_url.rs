//! The API origin every request is confined to.

use std::fmt;

use thiserror::Error;
use url::{Host, Url};

/// Postmark's production API origin.
pub const POSTMARK_API_ORIGIN: &str = "https://api.postmarkapp.com/";

/// A rejected API base URL or request path.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BaseUrlError {
    /// The value is not a URL.
    #[error("API base URL is not a valid URL: {0}")]
    Malformed(#[from] url::ParseError),
    /// Plain HTTP would expose the server token on the network.
    #[error("API base URL must use https, or http only on a loopback address")]
    InsecureScheme,
    /// Only a bare origin can be confined to reliably.
    #[error("API base URL must be a bare origin, with no credentials, path, query, or fragment")]
    NotBareOrigin,
    /// A resolved request would have left the API origin.
    #[error("refused a request path that resolves outside the API origin")]
    OutsideOrigin,
}

/// A validated API origin.
///
/// The server token is sent to this origin, so it must be `https`, or `http` on
/// a loopback address for local testing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiBaseUrl(Url);

impl ApiBaseUrl {
    /// Whether this origin uses TLS.
    #[must_use]
    pub fn is_https(&self) -> bool {
        self.0.scheme() == "https"
    }

    /// Resolves a request path against this origin.
    ///
    /// # Errors
    ///
    /// Returns [`BaseUrlError::OutsideOrigin`] when the path would reach another
    /// origin, as a protocol-relative `//host` path does, since that would send
    /// the server token to that host.
    pub fn resolve(&self, path_and_query: &str) -> Result<Url, BaseUrlError> {
        let resolved = self.0.join(path_and_query)?;
        if resolved.origin() == self.0.origin() {
            Ok(resolved)
        } else {
            Err(BaseUrlError::OutsideOrigin)
        }
    }
}

impl TryFrom<&str> for ApiBaseUrl {
    type Error = BaseUrlError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let url = Url::parse(value)?;
        let is_loopback = match url.host() {
            Some(Host::Domain(domain)) => domain == "localhost",
            Some(Host::Ipv4(address)) => address.is_loopback(),
            Some(Host::Ipv6(address)) => address.is_loopback(),
            None => false,
        };
        if url.scheme() != "https" && !(url.scheme() == "http" && is_loopback) {
            return Err(BaseUrlError::InsecureScheme);
        }
        let has_credentials = !url.username().is_empty() || url.password().is_some();
        if has_credentials || url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
            return Err(BaseUrlError::NotBareOrigin);
        }
        Ok(Self(url))
    }
}

impl fmt::Display for ApiBaseUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

#[cfg(test)]
#[path = "base_url_test.rs"]
mod tests;
