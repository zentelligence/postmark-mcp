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
fn requests_are_built_on_the_api_origin() -> Result<(), BaseUrlError> {
    let base = ApiBaseUrl::try_from(POSTMARK_API_ORIGIN)?;
    let query = QueryParams::default()
        .with("count", 10)
        .with_optional("tag", None::<&str>);

    assert_eq!(
        base.endpoint(&["messages", "outbound"], &query)?.as_str(),
        "https://api.postmarkapp.com/messages/outbound?count=10"
    );
    Ok(())
}

#[test]
fn requests_with_no_query_have_no_trailing_separator() -> Result<(), BaseUrlError> {
    let base = ApiBaseUrl::try_from(POSTMARK_API_ORIGIN)?;

    assert_eq!(
        base.endpoint(&["server"], &QueryParams::default())?.as_str(),
        "https://api.postmarkapp.com/server"
    );
    Ok(())
}

#[test]
fn values_shaped_like_urls_cannot_leave_the_api_origin() -> Result<(), BaseUrlError> {
    let base = ApiBaseUrl::try_from(POSTMARK_API_ORIGIN)?;

    for hostile in [
        "//evil.example.com/server",
        "https://evil.example.com/",
        "@evil.example.com",
        "a/../../b",
    ] {
        let url = base.endpoint(&["templates", hostile], &QueryParams::default())?;

        assert_eq!(url.host_str(), Some("api.postmarkapp.com"), "{hostile}");
        assert_eq!(url.path_segments().map(Iterator::count), Some(2), "{hostile}");
    }
    Ok(())
}

#[test]
fn query_values_cannot_add_parameters() -> Result<(), BaseUrlError> {
    let base = ApiBaseUrl::try_from(POSTMARK_API_ORIGIN)?;
    let query = QueryParams::default().with("tag", "a&count=500#b");

    let url = base.endpoint(&["bounces"], &query)?;

    assert_eq!(url.query_pairs().count(), 1);
    assert_eq!(url.fragment(), None);
    Ok(())
}
