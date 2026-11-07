//! HTTP transport for the Postmark API.
//!
//! Every property here protects the server token or bounds what a remote peer
//! can make this process do. Redirects are refused because custom headers such
//! as `X-Postmark-Server-Token` are carried across them, including cross-origin.

use std::time::Duration;

use postmark_mcp_core::{AgentLabel, GatewayError, ServerToken};
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, InvalidHeaderValue};
use reqwest::redirect::Policy;
use reqwest::{RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::base_url::{ApiBaseUrl, BaseUrlError, QueryParams};

/// Postmark's `ErrorCode` for a bad or missing server token.
const ERROR_CODE_BAD_TOKEN: i64 = 10;

/// Upper bound on Postmark error text relayed to callers, in characters.
const MAX_ERROR_MESSAGE_CHARS: usize = 500;

const SERVER_TOKEN_HEADER: HeaderName = HeaderName::from_static("x-postmark-server-token");
const CLIENT_HEADER: HeaderName = HeaderName::from_static("x-postmark-client");
const CLIENT_VERSION_HEADER: HeaderName = HeaderName::from_static("x-postmark-client-version");
const AGENT_LABEL_HEADER: HeaderName = HeaderName::from_static("x-agent-label");

/// Identifies this implementation to Postmark, distinct from the official
/// `ActiveCampaign` `postmark-mcp` so its traffic is not attributed to that project.
const CLIENT_NAME: &str = "zentelligence-postmark-mcp";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Connection settings for [`HardenedTransport`].
#[derive(Clone, Debug)]
pub struct TransportConfig {
    /// Origin every request is confined to.
    pub base_url: ApiBaseUrl,
    /// Token sent on every request.
    pub server_token: ServerToken,
    /// Optional label sent on every request as `X-Agent-Label`.
    pub agent_label: Option<AgentLabel>,
    /// Limit on a whole request, including reading the response.
    pub timeout: Duration,
    /// Limit on the size of a response body.
    pub max_response_bytes: usize,
}

impl TransportConfig {
    /// Default limit on a whole request.
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_mins(1);
    /// Default limit on a response body. Postmark caps a message at 10 MB
    /// before base64 encoding, so this leaves room for a full message dump.
    pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

    /// Creates settings with the default limits and no agent label.
    #[must_use]
    pub fn new(base_url: ApiBaseUrl, server_token: ServerToken) -> Self {
        Self {
            base_url,
            server_token,
            agent_label: None,
            timeout: Self::DEFAULT_TIMEOUT,
            max_response_bytes: Self::DEFAULT_MAX_RESPONSE_BYTES,
        }
    }
}

/// [`HardenedTransport`] could not be constructed.
#[derive(Debug, Error)]
pub enum TransportBuildError {
    /// The configured token or label could not be placed in a header.
    #[error("configured value cannot be sent as an HTTP header")]
    InvalidHeader(#[from] InvalidHeaderValue),
    /// The HTTP client could not be constructed.
    #[error("could not construct the HTTP client: {0}")]
    ClientConstruction(#[source] reqwest::Error),
}

/// An HTTP client for the Postmark API that confines requests to one origin,
/// refuses redirects, and bounds request time and response size.
#[derive(Clone, Debug)]
pub struct HardenedTransport {
    client: reqwest::Client,
    base_url: ApiBaseUrl,
    max_response_bytes: usize,
}

impl TryFrom<TransportConfig> for HardenedTransport {
    type Error = TransportBuildError;

    fn try_from(config: TransportConfig) -> Result<Self, Self::Error> {
        let mut token = HeaderValue::try_from(config.server_token.expose())?;
        token.set_sensitive(true);

        let mut headers = HeaderMap::new();
        headers.insert(SERVER_TOKEN_HEADER, token);
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CLIENT_HEADER, HeaderValue::from_static(CLIENT_NAME));
        headers.insert(CLIENT_VERSION_HEADER, HeaderValue::from_static(CLIENT_VERSION));
        if let Some(label) = &config.agent_label {
            headers.insert(AGENT_LABEL_HEADER, HeaderValue::try_from(label.as_ref())?);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent(concat!("zentelligence-postmark-mcp/", env!("CARGO_PKG_VERSION")))
            .redirect(Policy::none())
            .timeout(config.timeout)
            .https_only(config.base_url.is_https())
            .build()
            .map_err(TransportBuildError::ClientConstruction)?;

        Ok(Self {
            client,
            base_url: config.base_url,
            max_response_bytes: config.max_response_bytes,
        })
    }
}

impl HardenedTransport {
    /// Sends a `GET` and decodes the JSON response.
    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        query: &QueryParams,
    ) -> Result<T, GatewayError> {
        let url = self.base_url.endpoint(segments, query)?;
        self.execute(self.client.get(url)).await
    }

    /// Sends a `POST` with a JSON body and decodes the JSON response.
    pub(crate) async fn post<B: Serialize + Sync, T: DeserializeOwned>(
        &self,
        segments: &[&str],
        body: &B,
    ) -> Result<T, GatewayError> {
        let url = self.base_url.endpoint(segments, &QueryParams::default())?;
        let body = serde_json::to_vec(body).map_err(|error| GatewayError::InvalidRequest {
            detail: error.to_string(),
        })?;
        let request = self
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .body(body);
        self.execute(request).await
    }

    async fn execute<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, GatewayError> {
        let response = request.send().await.map_err(RequestFailure)?;

        let status = response.status();
        if status.is_redirection() {
            return Err(GatewayError::RedirectRefused {
                status: status.as_u16(),
            });
        }

        let body = self.read_body(response).await?;
        if !status.is_success() {
            return Err(Rejection { status, body: &body }.into());
        }
        // Not `error.to_string()`, which can quote values from the response.
        serde_json::from_slice(&body).map_err(|error| GatewayError::InvalidResponse {
            detail: format!("unexpected shape at line {}, column {}", error.line(), error.column()),
        })
    }

    async fn read_body(&self, mut response: reqwest::Response) -> Result<Vec<u8>, GatewayError> {
        // Enforced as chunks arrive rather than from `Content-Length`, because a
        // peer need not declare a length, or declare it truthfully.
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(RequestFailure)? {
            if body.len().saturating_add(chunk.len()) > self.max_response_bytes {
                return Err(GatewayError::ResponseTooLarge {
                    limit_bytes: self.max_response_bytes,
                });
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}

impl From<BaseUrlError> for GatewayError {
    fn from(error: BaseUrlError) -> Self {
        Self::InvalidRequest {
            detail: error.to_string(),
        }
    }
}

/// Local wrapper, since neither side of this conversion is defined here.
struct RequestFailure(reqwest::Error);

impl From<RequestFailure> for GatewayError {
    fn from(RequestFailure(error): RequestFailure) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else {
            Self::Unreachable {
                detail: error.without_url().to_string(),
            }
        }
    }
}

/// A non-success response from Postmark.
struct Rejection<'body> {
    status: StatusCode,
    body: &'body [u8],
}

/// Error body Postmark returns on failure. Absent or unparsable on some
/// failures, such as those from infrastructure in front of the API.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct ErrorBody {
    error_code: Option<i64>,
    message: Option<String>,
}

impl From<Rejection<'_>> for GatewayError {
    fn from(Rejection { status, body }: Rejection<'_>) -> Self {
        let ErrorBody { error_code, message } = serde_json::from_slice(body).unwrap_or_default();

        if status == StatusCode::UNAUTHORIZED || error_code == Some(ERROR_CODE_BAD_TOKEN) {
            return Self::Unauthorised;
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Self::RateLimited;
        }
        if status.is_server_error() {
            return Self::Unavailable {
                status: status.as_u16(),
            };
        }
        Self::Rejected {
            status: status.as_u16(),
            error_code,
            message: message
                .map(|text| text.chars().take(MAX_ERROR_MESSAGE_CHARS).collect())
                .unwrap_or_default(),
        }
    }
}
