//! Implements the domain ports against the Postmark API.
//!
//! Wire types here model only the response fields this server uses. Anything
//! else in a response is discarded during decoding, which is how `ApiTokens` in
//! the `/server` response stays out of this process's data entirely.

use async_trait::async_trait;
use postmark_mcp_core::{GatewayError, ReadsServer, ServerSummary};
use reqwest::Method;
use serde::Deserialize;
use url::Url;

use crate::transport::HardenedTransport;

/// The Postmark API, reached through a [`HardenedTransport`].
#[derive(Clone, Debug)]
pub struct PostmarkApi {
    transport: HardenedTransport,
}

impl From<HardenedTransport> for PostmarkApi {
    fn from(transport: HardenedTransport) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl ReadsServer for PostmarkApi {
    async fn current_server(&self) -> Result<ServerSummary, GatewayError> {
        let server: ServerResponse = self.transport.request(Method::GET, "/server").await?;
        Ok(server.into())
    }
}

/// Response of `GET /server`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ServerResponse {
    #[serde(rename = "ID")]
    id: i64,
    name: String,
    color: Option<String>,
    smtp_api_activated: Option<bool>,
    inbound_address: Option<String>,
    inbound_domain: Option<String>,
    track_opens: Option<bool>,
    track_links: Option<String>,
    post_first_open_only: Option<bool>,
    bounce_hook_url: Option<String>,
    open_hook_url: Option<String>,
    delivery_hook_url: Option<String>,
    click_hook_url: Option<String>,
    inbound_hook_url: Option<String>,
}

impl From<ServerResponse> for ServerSummary {
    fn from(server: ServerResponse) -> Self {
        Self {
            id: server.id,
            name: server.name,
            color: present(server.color),
            smtp_api_activated: server.smtp_api_activated,
            inbound_address: present(server.inbound_address),
            inbound_domain: present(server.inbound_domain),
            track_opens: server.track_opens,
            track_links: present(server.track_links),
            post_first_open_only: server.post_first_open_only,
            bounce_hook_url: without_credentials(server.bounce_hook_url),
            open_hook_url: without_credentials(server.open_hook_url),
            delivery_hook_url: without_credentials(server.delivery_hook_url),
            click_hook_url: without_credentials(server.click_hook_url),
            inbound_hook_url: without_credentials(server.inbound_hook_url),
        }
    }
}

/// Postmark reports an unset string as empty rather than null.
fn present(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
}

/// Removes basic auth credentials, which a hook URL may legitimately embed but
/// which must not reach a tool result. A value that cannot be parsed could hide
/// credentials in a form this cannot see, so it is withheld entirely.
fn without_credentials(value: Option<String>) -> Option<String> {
    let mut url = Url::parse(&present(value)?).ok()?;
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    Some(url.into())
}
