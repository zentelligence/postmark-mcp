//! The API origin every request is confined to, and how request URLs are built
//! on it.

use std::fmt;

use thiserror::Error;
use url::{Host, Url};

/// Postmark's production API origin.
pub const POSTMARK_API_ORIGIN: &str = "https://api.postmarkapp.com/";

/// A rejected API base URL.
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

    /// Builds a request URL on this origin.
    ///
    /// Segments and query values are percent-encoded here, so a value can only
    /// ever be one path segment or one query value. Nothing replaces the host,
    /// which is what keeps the server token on this origin.
    ///
    /// # Errors
    ///
    /// Returns [`BaseUrlError::NotBareOrigin`] if the origin cannot take a path,
    /// which validation at construction prevents.
    pub(crate) fn endpoint(&self, segments: &[&str], query: &QueryParams) -> Result<Url, BaseUrlError> {
        let mut url = self.0.clone();
        url.path_segments_mut()
            .map_err(|()| BaseUrlError::NotBareOrigin)?
            .extend(segments);
        if !query.0.is_empty() {
            url.query_pairs_mut().extend_pairs(&query.0);
        }
        Ok(url)
    }
}

/// Query parameters for a request.
#[derive(Clone, Debug, Default)]
pub(crate) struct QueryParams(Vec<(&'static str, String)>);

impl QueryParams {
    /// Adds a parameter.
    pub(crate) fn with(mut self, name: &'static str, value: impl fmt::Display) -> Self {
        self.0.push((name, value.to_string()));
        self
    }

    /// Adds a parameter when it has a value.
    pub(crate) fn with_optional(self, name: &'static str, value: Option<impl fmt::Display>) -> Self {
        match value {
            Some(value) => self.with(name, value),
            None => self,
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
