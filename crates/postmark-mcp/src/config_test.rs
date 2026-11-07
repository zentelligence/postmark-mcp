use super::*;

fn environment(extra: &[(&str, &str)]) -> Environment {
    [
        (SERVER_TOKEN.variable, "test-token"),
        (DEFAULT_SENDER.variable, "sender@example.com"),
        (DEFAULT_MESSAGE_STREAM.variable, "outbound"),
    ]
    .iter()
    .chain(extra)
    .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
    .collect()
}

fn from_environment(environment: &Environment) -> Result<Config, ConfigError> {
    Config::try_from(Sources {
        settings: &Settings::default(),
        environment,
    })
}

fn from_flags(settings: &Settings) -> Result<Config, ConfigError> {
    Config::try_from(Sources {
        settings,
        environment: &Environment::default(),
    })
}

fn required_flags() -> Settings {
    Settings {
        server_token: Some("flag-token".to_owned()),
        default_sender: Some("flag@example.com".to_owned()),
        default_message_stream: Some("flag-stream".to_owned()),
        ..Settings::default()
    }
}

#[test]
fn required_variables_alone_give_production_defaults() -> Result<(), ConfigError> {
    let config = from_environment(&environment(&[]))?;

    assert_eq!(config.server_token.expose(), "test-token");
    assert_eq!(config.default_sender.as_ref(), "sender@example.com");
    assert_eq!(config.default_message_stream.as_ref(), "outbound");
    assert_eq!(config.agent_label, None);
    assert_eq!(config.api_base_url.to_string(), POSTMARK_API_ORIGIN);
    assert!(!config.skip_verify);
    assert_eq!(config.toolsets, Toolsets::default());
    Ok(())
}

#[test]
fn flags_alone_are_enough() -> Result<(), ConfigError> {
    let config = from_flags(&required_flags())?;

    assert_eq!(config.server_token.expose(), "flag-token");
    assert_eq!(config.default_sender.as_ref(), "flag@example.com");
    assert_eq!(config.default_message_stream.as_ref(), "flag-stream");
    Ok(())
}

#[test]
fn a_flag_wins_over_its_variable() -> Result<(), ConfigError> {
    let config = Config::try_from(Sources {
        settings: &required_flags(),
        environment: &environment(&[(TOOLSETS.variable, "read")]),
    })?;

    assert_eq!(config.server_token.expose(), "flag-token");
    assert_eq!(config.default_sender.as_ref(), "flag@example.com");
    assert!(
        config.toolsets.read && !config.toolsets.send,
        "the variable applies where no flag is given"
    );
    Ok(())
}

#[test]
fn optional_settings_are_applied() -> Result<(), ConfigError> {
    let config = from_environment(&environment(&[
        (AGENT_LABEL.variable, "ci"),
        (API_BASE_URL.variable, "http://127.0.0.1:9000"),
        (SKIP_VERIFY.variable, "true"),
        (LOG_EMAIL_FULL.variable, "true"),
        (LOG_FILE.variable, "/tmp/log"),
    ]))?;

    assert_eq!(config.agent_label.as_ref().map(AsRef::as_ref), Some("ci"));
    assert_eq!(config.api_base_url.to_string(), "http://127.0.0.1:9000/");
    assert!(config.skip_verify);
    assert!(config.log_email_full);
    assert_eq!(config.log_file, Some(PathBuf::from("/tmp/log")));
    Ok(())
}

#[test]
fn switches_are_on_when_their_flag_is_present() -> Result<(), ConfigError> {
    let config = from_flags(&Settings {
        skip_verify: true,
        log_email_full: true,
        ..required_flags()
    })?;

    assert!(config.skip_verify);
    assert!(config.log_email_full);
    Ok(())
}

#[test]
fn each_required_setting_is_reported_by_flag_and_variable() {
    for required in [SERVER_TOKEN, DEFAULT_SENDER, DEFAULT_MESSAGE_STREAM] {
        let without: Environment = environment(&[])
            .0
            .into_iter()
            .filter(|(name, _)| name != required.variable)
            .collect();
        let blank = environment(&[(required.variable, "")]);

        for candidate in [without, blank] {
            let error = from_environment(&candidate).err();
            let message = error.as_ref().map(ToString::to_string).unwrap_or_default();

            assert!(
                matches!(error, Some(ConfigError::Missing { setting, .. }) if setting == required),
                "{required}"
            );
            assert!(
                message.contains(required.flag) && message.contains(required.variable),
                "{message}"
            );
        }
    }
}

#[test]
fn blank_optional_variables_count_as_unset() -> Result<(), ConfigError> {
    let config = from_environment(&environment(&[
        (AGENT_LABEL.variable, ""),
        (API_BASE_URL.variable, ""),
        (SKIP_VERIFY.variable, ""),
        (TOOLSETS.variable, ""),
    ]))?;

    assert_eq!(config.agent_label, None);
    assert_eq!(config.api_base_url.to_string(), POSTMARK_API_ORIGIN);
    assert!(!config.skip_verify);
    assert_eq!(config.toolsets, Toolsets::default());
    Ok(())
}

#[test]
fn toolsets_are_read_as_a_comma_separated_list() -> Result<(), ConfigError> {
    assert_eq!(
        Toolsets::try_from(" read , manage ")?,
        Toolsets {
            read: true,
            send: false,
            manage: true
        }
    );
    for unusable in ["", ",", "read,write", "all", "READ"] {
        assert!(
            matches!(Toolsets::try_from(unusable), Err(ConfigError::UnknownToolset { .. })),
            "{unusable:?}"
        );
    }
    Ok(())
}

#[test]
fn an_empty_toolsets_flag_is_refused_rather_than_read_as_the_default() {
    let result = from_flags(&Settings {
        toolsets: Some(String::new()),
        ..required_flags()
    });

    assert!(matches!(result, Err(ConfigError::UnknownToolset { .. })));
}

#[test]
fn invalid_values_name_the_setting_without_echoing_a_token() {
    let error = from_environment(&environment(&[(SERVER_TOKEN.variable, "leaky token")])).err();

    assert!(error.is_some_and(|error| {
        let message = error.to_string();
        message.contains(SERVER_TOKEN.flag) && message.contains(SERVER_TOKEN.variable) && !message.contains("leaky")
    }));
}

#[test]
fn insecure_base_urls_are_rejected() {
    let result = from_environment(&environment(&[(API_BASE_URL.variable, "http://api.postmarkapp.com")]));

    assert!(matches!(
        result,
        Err(ConfigError::BaseUrl(BaseUrlError::InsecureScheme))
    ));
}

#[test]
fn unrecognised_switch_values_are_rejected_rather_than_read_as_false() {
    let result = from_environment(&environment(&[(SKIP_VERIFY.variable, "yes")]));

    assert!(matches!(result, Err(ConfigError::InvalidFlag { setting }) if setting == SKIP_VERIFY));
}

#[test]
fn config_debug_output_never_contains_the_token() -> Result<(), ConfigError> {
    let config = from_environment(&environment(&[]))?;

    assert!(!format!("{config:?}").contains("test-token"));
    Ok(())
}

#[test]
fn settings_debug_output_never_contains_the_token() {
    let settings = Settings {
        server_token: Some("flag-secret".to_owned()),
        ..Settings::default()
    };

    assert!(!format!("{settings:?}").contains("flag-secret"));
}
