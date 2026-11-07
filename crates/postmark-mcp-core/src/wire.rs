//! Shared decoding rules for Postmark responses, and the macro behind the
//! fixed-vocabulary enums.

use serde::{Deserialize, Deserializer};
use url::Url;

/// Free-form JSON from Postmark or a caller, such as metadata or a render model.
pub type JsonObject = serde_json::Map<String, serde_json::Value>;

/// Decodes a string Postmark may report as empty when unset.
pub(crate) fn blank_as_none<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(value.filter(|text| !text.is_empty()))
}

/// Decodes a value Postmark may report as `null`, such as an empty event list.
pub(crate) fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// Decodes optional free-form data, treating any other shape as absent. Postmark
/// documents `Metadata` as an array but shows an object, and an optional field
/// must not fail a whole response.
pub(crate) fn object_or_none<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<JsonObject>, D::Error> {
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Object(object) => Some(object),
        _ => None,
    })
}

/// Event detail fields safe to relay. An allowlist, because `Details` varies by
/// event, and can hold link URLs, geolocation, and client addresses.
const SAFE_EVENT_DETAILS: [&str; 8] = [
    "BounceID",
    "ClickLocation",
    "DeliveryMessage",
    "DestinationIP",
    "DestinationServer",
    "Origin",
    "Summary",
    "SuppressSending",
];

/// Decodes event details, keeping only safe fields. A value holding a URL is
/// dropped too, because a `LinkClicked` summary repeats the link, and a clicked
/// link can be a live password reset token.
pub(crate) fn safe_event_details<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<JsonObject>, D::Error> {
    Ok(object_or_none(deserializer)?.map(|details| {
        details
            .into_iter()
            .filter(|(name, value)| {
                SAFE_EVENT_DETAILS.contains(&name.as_str()) && !value.as_str().is_some_and(|text| text.contains("://"))
            })
            .collect()
    }))
}

/// Decodes a URL with its credentials, query, and fragment removed, since each
/// commonly carries a secret that must not reach a tool result. A secret in the
/// path cannot be told from an ordinary path, so paths are kept. A value that
/// cannot be parsed could hide credentials in a form this cannot see, so it is
/// withheld entirely.
pub(crate) fn sanitised_url<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(blank_as_none(deserializer)?.and_then(|value| {
        let mut url = Url::parse(&value).ok()?;
        url.set_username("").ok()?;
        url.set_password(None).ok()?;
        url.set_query(None);
        url.set_fragment(None);
        Some(url.into())
    }))
}

/// Defines an enum over a fixed Postmark vocabulary, and its wire value.
macro_rules! vocabulary {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
        #[schemars(inline)]
        pub enum $name {
            #[default]
            $(#[serde(rename = $wire)] $variant,)+
        }

        impl $name {
            /// The value Postmark uses for this variant.
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire,)+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

pub(crate) use vocabulary;
