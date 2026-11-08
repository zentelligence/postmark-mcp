//! The launcher's own settings, read from its environment and validated before
//! anything is fetched or written.

use std::ffi::OsString;

use updater_core::ProductVersion;
use updater_engine::TrustedKey;

use crate::failure::Failure;
use crate::product::{DEFAULT_UPDATE_URL, TrustedPublicKey};

/// The variable that points the launcher at another signed manifest, such as a
/// release that is not yet the latest.
pub(crate) const UPDATE_URL_VARIABLE: &str = "POSTMARK_MCP_UPDATE_URL";

/// Where the signed manifest is fetched from. Whether it is HTTPS is checked by
/// the updater itself, before any request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UpdateUrl(String);

impl UpdateUrl {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// The value of [`UPDATE_URL_VARIABLE`]. Unset means the published releases. An
/// empty value is refused, unlike the server's own variables, because treating
/// it as unset would quietly switch the source of what gets installed.
impl TryFrom<Option<OsString>> for UpdateUrl {
    type Error = Failure;

    fn try_from(value: Option<OsString>) -> Result<Self, Self::Error> {
        let Some(value) = value else {
            return Ok(Self(DEFAULT_UPDATE_URL.to_owned()));
        };
        let value = value.into_string().map_err(|_| Failure::UpdateUrlNotUnicode)?;
        if value.is_empty() {
            return Err(Failure::EmptyUpdateUrl);
        }
        Ok(Self(value))
    }
}

/// A release asked for with `--version`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PinnedVersion(pub(crate) ProductVersion);

impl TryFrom<&str> for PinnedVersion {
    type Error = Failure;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        ProductVersion::try_from(value)
            .map(Self)
            .map_err(Failure::MalformedPinnedVersion)
    }
}

impl TryFrom<&TrustedPublicKey> for TrustedKey {
    type Error = Failure;

    fn try_from(key: &TrustedPublicKey) -> Result<Self, Self::Error> {
        let malformed = || Failure::MalformedTrustedKey { id: key.id };
        let digits = key.hex.as_bytes();
        if digits.len() != 64 {
            return Err(malformed());
        }
        let (pairs, _) = digits.as_chunks::<2>();
        let mut bytes = [0_u8; 32];
        for (byte, [high, low]) in bytes.iter_mut().zip(pairs) {
            let nibble = |digit: &u8| char::from(*digit).to_digit(16);
            let (Some(high), Some(low)) = (nibble(high), nibble(low)) else {
                return Err(malformed());
            };
            *byte = u8::try_from(high * 16 + low).map_err(|_| malformed())?;
        }
        TrustedKey::new(key.id, &bytes).map_err(|_| malformed())
    }
}

#[cfg(test)]
#[path = "settings_test.rs"]
mod tests;
