use super::*;

fn environment(extra: &[(&str, &str)]) -> Environment {
    [
        (SERVER_TOKEN, "test-token"),
        (DEFAULT_SENDER, "sender@example.com"),
        (DEFAULT_MESSAGE_STREAM, "outbound"),
    ]
    .iter()
    .chain(extra)
    .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
    .collect()
}

#[test]
fn required_variables_alone_give_production_defaults() -> Result<(), ConfigError> {
    let config = Config::try_from(&environment(&[]))?;

    assert_eq!(config.server_token.expose(), "test-token");
    assert_eq!(config.default_sender.as_ref(), "sender@example.com");
    assert_eq!(config.default_message_stream.as_ref(), "outbound");
    assert_eq!(config.agent_label, None);
    assert_eq!(config.api_base_url.to_string(), POSTMARK_API_ORIGIN);
    assert!(!config.skip_verify);
    Ok(())
}

#[test]
fn optional_variables_are_applied() -> Result<(), ConfigError> {
    let config = Config::try_from(&environment(&[
        (AGENT_LABEL, "ci"),
        (API_BASE_URL, "http://127.0.0.1:9000"),
        (SKIP_VERIFY, "true"),
    ]))?;

    assert_eq!(config.agent_label.as_ref().map(AsRef::as_ref), Some("ci"));
    assert_eq!(config.api_base_url.to_string(), "http://127.0.0.1:9000/");
    assert!(config.skip_verify);
    Ok(())
}

#[test]
fn each_required_variable_is_reported_when_unset_or_empty() {
    for required in [SERVER_TOKEN, DEFAULT_SENDER, DEFAULT_MESSAGE_STREAM] {
        let without: Environment = environment(&[])
            .0
            .into_iter()
            .filter(|(name, _)| name != required)
            .collect();
        let blank = environment(&[(required, "")]);

        for candidate in [without, blank] {
            assert!(
                matches!(Config::try_from(&candidate), Err(ConfigError::Missing { name, .. }) if name == required),
                "{required}"
            );
        }
    }
}

#[test]
fn blank_optional_variables_count_as_unset() -> Result<(), ConfigError> {
    let config = Config::try_from(&environment(&[
        (AGENT_LABEL, ""),
        (API_BASE_URL, ""),
        (SKIP_VERIFY, ""),
    ]))?;

    assert_eq!(config.agent_label, None);
    assert_eq!(config.api_base_url.to_string(), POSTMARK_API_ORIGIN);
    assert!(!config.skip_verify);
    Ok(())
}

#[test]
fn invalid_values_name_the_variable_without_echoing_a_token() {
    let error = Config::try_from(&environment(&[(SERVER_TOKEN, "leaky token")])).err();

    assert!(error.is_some_and(|error| {
        let message = error.to_string();
        message.contains(SERVER_TOKEN) && !message.contains("leaky")
    }));
}

#[test]
fn insecure_base_urls_are_rejected() {
    let result = Config::try_from(&environment(&[(API_BASE_URL, "http://api.postmarkapp.com")]));

    assert!(matches!(
        result,
        Err(ConfigError::BaseUrl(BaseUrlError::InsecureScheme))
    ));
}

#[test]
fn unrecognised_flag_values_are_rejected_rather_than_read_as_false() {
    let result = Config::try_from(&environment(&[(SKIP_VERIFY, "yes")]));

    assert!(matches!(result, Err(ConfigError::InvalidFlag { name: SKIP_VERIFY })));
}

#[test]
fn config_debug_output_never_contains_the_token() -> Result<(), ConfigError> {
    let config = Config::try_from(&environment(&[]))?;

    assert!(!format!("{config:?}").contains("test-token"));
    Ok(())
}
