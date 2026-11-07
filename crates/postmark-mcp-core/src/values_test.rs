use super::*;

#[test]
fn server_token_debug_output_never_contains_the_secret() -> Result<(), ValueError> {
    let token = ServerToken::try_from("super-secret-token".to_owned())?;

    assert!(!format!("{token:?}").contains("super-secret-token"));
    assert_eq!(token.expose(), "super-secret-token");
    Ok(())
}

#[test]
fn server_token_rejects_values_that_cannot_be_sent_as_a_header() {
    for (input, expected) in [
        ("", ValueError::Empty { field: "server token" }),
        ("   ", ValueError::Empty { field: "server token" }),
        (
            "token\r\nX-Injected: yes",
            ValueError::NotHeaderSafe { field: "server token" },
        ),
        ("tok en", ValueError::NotHeaderSafe { field: "server token" }),
        ("tökén", ValueError::NotHeaderSafe { field: "server token" }),
    ] {
        assert_eq!(
            ServerToken::try_from(input.to_owned()).err(),
            Some(expected),
            "{input:?}"
        );
    }
}

#[test]
fn value_errors_do_not_echo_the_rejected_input() {
    let error = ServerToken::try_from("leaky secret".to_owned()).err();

    assert!(error.is_some_and(|error| !error.to_string().contains("leaky")));
}

#[test]
fn sender_accepts_bare_and_named_addresses() {
    for input in ["sender@example.com", "Support Team <sender@example.com>"] {
        assert!(SenderAddress::try_from(input.to_owned()).is_ok(), "{input:?}");
    }
}

#[test]
fn sender_rejects_malformed_and_header_injecting_values() {
    for input in [
        "no-at-sign",
        "@example.com",
        "sender@",
        "a@example.com\r\nBcc: b@example.com",
    ] {
        assert_eq!(
            SenderAddress::try_from(input.to_owned()),
            Err(ValueError::MalformedSender),
            "{input:?}"
        );
    }
    assert_eq!(
        SenderAddress::try_from(" ".to_owned()),
        Err(ValueError::Empty { field: "sender" })
    );
}

#[test]
fn message_stream_accepts_postmark_identifiers() {
    for input in ["outbound", "broadcast-1", "my_stream"] {
        assert!(MessageStreamId::try_from(input.to_owned()).is_ok(), "{input:?}");
    }
}

#[test]
fn message_stream_rejects_values_that_could_alter_a_request_path() {
    for input in ["../servers", "a/b", "a b", "a?b=c"] {
        assert_eq!(
            MessageStreamId::try_from(input.to_owned()),
            Err(ValueError::MalformedMessageStream),
            "{input:?}"
        );
    }
    assert_eq!(
        MessageStreamId::try_from(String::new()),
        Err(ValueError::Empty {
            field: "message stream ID"
        })
    );
}

#[test]
fn agent_label_is_bounded_and_header_safe() {
    assert!(AgentLabel::try_from("claude desktop".to_owned()).is_ok());
    assert!(AgentLabel::try_from("x".repeat(MAX_AGENT_LABEL_LEN)).is_ok());
    assert_eq!(
        AgentLabel::try_from("x".repeat(MAX_AGENT_LABEL_LEN + 1)),
        Err(ValueError::TooLong {
            field: "agent label",
            limit: MAX_AGENT_LABEL_LEN
        })
    );
    assert_eq!(
        AgentLabel::try_from("label\nX-Injected: yes".to_owned()),
        Err(ValueError::NotHeaderSafe { field: "agent label" })
    );
}

#[test]
fn filter_text_rejects_blank_control_and_oversized_values() {
    let filter = |value: String| FilterText::try_from(("tag", value));

    assert!(filter("welcome".to_owned()).is_ok());
    assert!(filter("x".repeat(MAX_FILTER_LEN)).is_ok());
    assert_eq!(filter(" ".to_owned()), Err(ValueError::Empty { field: "tag" }));
    assert_eq!(
        filter("a\nb".to_owned()),
        Err(ValueError::ControlCharacters { field: "tag" })
    );
    assert_eq!(
        filter("x".repeat(MAX_FILTER_LEN + 1)),
        Err(ValueError::TooLong {
            field: "tag",
            limit: MAX_FILTER_LEN
        })
    );
}

#[test]
fn resource_id_rejects_what_url_normalisation_would_alter() {
    let id = |value: &str| ResourceId::try_from(("id", value.to_owned()));

    assert!(id("welcome-email").is_ok());
    assert!(id("a/b").is_ok(), "encoded later, so it stays one path segment");
    assert_eq!(id(""), Err(ValueError::Empty { field: "id" }));
    for value in [".", "..", "a\nb"] {
        assert_eq!(
            id(value),
            Err(ValueError::InvalidIdentifier { field: "id" }),
            "{value:?}"
        );
    }
}

#[test]
fn bounce_id_must_be_positive() {
    assert!(BounceId::try_from(1).is_ok());
    assert_eq!(
        BounceId::try_from(0),
        Err(ValueError::InvalidIdentifier { field: "bounce_id" })
    );
}

#[test]
fn dates_must_exist_on_the_calendar() {
    let date = |value: &str| PostmarkDate::try_from(("from_date", value.to_owned()));

    for value in ["2026-01-31", "2024-02-29", "2000-02-29", "2026-04-30"] {
        assert!(date(value).is_ok(), "{value}");
    }
    for value in [
        "2026-02-29",
        "1900-02-29",
        "2026-04-31",
        "2026-13-01",
        "2026-00-10",
        "2026-01-00",
        "2026-1-1",
        "26-01-01",
        "2026-01-01T00:00:00",
        "+026-01-01",
        "",
    ] {
        assert_eq!(
            date(value),
            Err(ValueError::InvalidDate { field: "from_date" }),
            "{value}"
        );
    }
}

#[test]
fn date_ranges_may_be_open_or_a_single_day_but_not_inverted() {
    let range =
        |from: Option<&str>, to: Option<&str>| DateRange::try_from((from.map(str::to_owned), to.map(str::to_owned)));

    assert!(range(None, None).is_ok());
    assert!(range(Some("2026-01-01"), None).is_ok());
    assert!(range(None, Some("2026-01-01")).is_ok());
    assert!(range(Some("2026-01-01"), Some("2026-01-01")).is_ok());
    assert_eq!(
        range(Some("2026-01-02"), Some("2026-01-01")),
        Err(ValueError::InvertedDateRange)
    );
}

#[test]
fn pages_apply_defaults_and_postmarks_limits() {
    let page = |count, offset| {
        Page::try_from(PageRequest {
            count,
            offset,
            default_count: 50,
        })
    };

    assert_eq!(page(None, None), Ok(Page { count: 50, offset: 0 }));
    assert_eq!(
        page(Some(500), Some(9500)),
        Ok(Page {
            count: 500,
            offset: 9500
        })
    );
    assert_eq!(page(Some(0), None), Err(ValueError::PageCountOutOfRange));
    assert_eq!(page(Some(501), None), Err(ValueError::PageCountOutOfRange));
    assert_eq!(page(Some(500), Some(9501)), Err(ValueError::PageTooDeep));
    assert_eq!(page(None, Some(u32::MAX)), Err(ValueError::PageTooDeep));
}

#[test]
fn recipient_must_be_one_bare_address() {
    for valid in ["alice@example.com", "first.last+tag@sub.example.com"] {
        assert!(RecipientAddress::try_from(valid.to_owned()).is_ok(), "{valid}");
    }
    for invalid in [
        "",
        "alice",
        "@example.com",
        "alice@",
        "a@b@c",
        "Alice <alice@example.com>",
        "a@b.com, c@d.com",
        "a b@example.com",
    ] {
        assert_eq!(
            RecipientAddress::try_from(invalid.to_owned()),
            Err(ValueError::MalformedRecipient),
            "{invalid}"
        );
    }
}

#[test]
fn recipient_matching_ignores_case_but_not_partial_addresses() -> Result<(), ValueError> {
    let recipient = RecipientAddress::try_from("alice@example.com".to_owned())?;

    assert!(recipient.matches("Alice@Example.COM"));
    assert!(recipient.matches(" alice@example.com "));
    assert!(!recipient.matches("malice@example.com"));
    assert!(!recipient.matches("alice@example.com.au"));
    Ok(())
}

#[test]
fn recipient_matching_sees_through_display_names() -> Result<(), ValueError> {
    let recipient = RecipientAddress::try_from("alice@example.com".to_owned())?;

    assert!(recipient.matches("Alice Smith <alice@example.com>"));
    assert!(recipient.matches("<ALICE@example.com>"));
    assert!(!recipient.matches("Alice <malice@example.com>"));
    assert!(!recipient.matches("alice@example.com <malice@example.com>"));
    Ok(())
}
