use super::*;
use crate::product::TRUSTED_KEYS;

#[test]
fn an_unset_update_url_is_the_published_releases() -> Result<(), Failure> {
    assert_eq!(UpdateUrl::try_from(None)?.as_str(), DEFAULT_UPDATE_URL);
    Ok(())
}

#[test]
fn a_set_update_url_is_taken_as_given() -> Result<(), Failure> {
    let url = UpdateUrl::try_from(Some(OsString::from("https://example.com/latest.json")))?;

    assert_eq!(url.as_str(), "https://example.com/latest.json");
    Ok(())
}

#[test]
fn an_empty_update_url_is_refused() {
    assert!(matches!(
        UpdateUrl::try_from(Some(OsString::new())),
        Err(Failure::EmptyUpdateUrl)
    ));
}

#[test]
fn the_published_releases_are_served_over_https() {
    assert!(DEFAULT_UPDATE_URL.starts_with("https://"));
}

#[test]
fn a_pinned_version_must_be_a_bare_release_number() {
    assert!(PinnedVersion::try_from("1.2.3").is_ok());
    for malformed in ["v1.2.3", "1.2", "", "latest"] {
        assert!(
            matches!(
                PinnedVersion::try_from(malformed),
                Err(Failure::MalformedPinnedVersion(_))
            ),
            "{malformed}"
        );
    }
}

#[test]
fn every_built_in_trusted_key_is_well_formed() {
    for key in &TRUSTED_KEYS {
        assert!(TrustedKey::try_from(key).is_ok(), "{}", key.id);
    }
}

#[test]
fn a_malformed_trusted_key_is_refused() {
    for hex in [
        "",
        "00",
        &"g".repeat(64),
        &"0".repeat(63),
        &"0".repeat(65),
        &"é".repeat(32),
    ] {
        let key = TrustedPublicKey {
            id: "test",
            hex: Box::leak(hex.to_owned().into_boxed_str()),
        };
        assert!(
            matches!(
                TrustedKey::try_from(&key),
                Err(Failure::MalformedTrustedKey { id: "test" })
            ),
            "{hex}"
        );
    }
}
