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
