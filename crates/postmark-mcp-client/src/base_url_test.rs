use super::*;

#[test]
fn production_origin_is_accepted() -> Result<(), BaseUrlError> {
    let base = ApiBaseUrl::try_from(POSTMARK_API_ORIGIN)?;

    assert!(base.is_https());
    assert_eq!(base.to_string(), POSTMARK_API_ORIGIN);
    Ok(())
}

#[test]
fn plain_http_is_accepted_only_on_loopback() {
    for accepted in ["http://127.0.0.1:8080", "http://localhost:8080", "http://[::1]:8080"] {
        assert!(ApiBaseUrl::try_from(accepted).is_ok(), "{accepted}");
    }
    for rejected in [
        "http://api.postmarkapp.com",
        "http://localhost.example.com",
        "http://127.0.0.1.example.com",
        "ftp://127.0.0.1",
    ] {
        assert_eq!(
            ApiBaseUrl::try_from(rejected),
            Err(BaseUrlError::InsecureScheme),
            "{rejected}"
        );
    }
}

#[test]
fn anything_beyond_a_bare_origin_is_rejected() {
    for rejected in [
        "https://user:secret@api.postmarkapp.com",
        "https://user@api.postmarkapp.com",
        "https://api.postmarkapp.com/v1",
        "https://api.postmarkapp.com/?a=b",
        "https://api.postmarkapp.com/#fragment",
    ] {
        assert_eq!(
            ApiBaseUrl::try_from(rejected),
            Err(BaseUrlError::NotBareOrigin),
            "{rejected}"
        );
    }
}

#[test]
fn malformed_urls_are_rejected() {
    assert!(matches!(
        ApiBaseUrl::try_from("not a url"),
        Err(BaseUrlError::Malformed(_))
    ));
}

#[test]
fn request_paths_resolve_on_the_api_origin() -> Result<(), BaseUrlError> {
    let base = ApiBaseUrl::try_from(POSTMARK_API_ORIGIN)?;

    assert_eq!(
        base.resolve("/messages/outbound?count=10")?.as_str(),
        "https://api.postmarkapp.com/messages/outbound?count=10"
    );
    Ok(())
}

#[test]
fn request_paths_cannot_escape_the_api_origin() -> Result<(), BaseUrlError> {
    let base = ApiBaseUrl::try_from(POSTMARK_API_ORIGIN)?;

    for escaping in [
        "//evil.example.com/server",
        "https://evil.example.com/server",
        "http://api.postmarkapp.com/",
    ] {
        assert_eq!(base.resolve(escaping), Err(BaseUrlError::OutsideOrigin), "{escaping}");
    }
    Ok(())
}
