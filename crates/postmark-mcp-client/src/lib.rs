//! Adapter from the domain ports in `postmark-mcp-core` to the Postmark API.
//!
//! Every request goes through [`HardenedTransport`], and [`PostmarkApi`] holds
//! the wire types and implements the ports on top of it.

mod api;
mod base_url;
mod transport;

pub use api::PostmarkApi;
pub use base_url::{ApiBaseUrl, BaseUrlError, POSTMARK_API_ORIGIN};
pub use transport::{HardenedTransport, TransportBuildError, TransportConfig};
