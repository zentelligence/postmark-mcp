use super::*;

fn url(value: &str) -> Result<WebhookUrl, ValueError> {
    WebhookUrl::try_from(value.to_owned())
}

#[test]
fn webhook_urls_must_be_https_without_credentials() {
    assert!(url("https://hooks.example.com/postmark").is_ok());
    for value in [
        "http://hooks.example.com/postmark",
        "https://user:pass@hooks.example.com/postmark",
        "https://user@hooks.example.com/",
        "not a url",
        "ftp://hooks.example.com/",
    ] {
        assert_eq!(url(value), Err(ValueError::InsecureWebhookUrl), "{value}");
    }
}

#[test]
fn webhook_allowlists_match_origin_and_whole_path_segments() -> Result<(), ValueError> {
    let list = WebhookUrlAllowlist::try_from("https://hooks.example.com/postmark, https://other.example.com")?;

    for permitted in [
        "https://hooks.example.com/postmark",
        "https://hooks.example.com/postmark/events",
        "https://HOOKS.example.com/postmark",
        "https://hooks.example.com:443/postmark",
        "https://other.example.com/anything",
    ] {
        assert!(list.permits(&url(permitted)?), "{permitted}");
    }
    for refused in [
        "https://hooks.example.com/postmark-other",
        "https://hooks.example.com/",
        "https://hooks.example.com:8443/postmark",
        "https://evil.example.com/postmark",
        "https://hooks.example.com.evil.com/postmark",
    ] {
        assert!(!list.permits(&url(refused)?), "{refused}");
    }
    Ok(())
}

#[test]
fn webhook_allowlists_refuse_unusable_entries() {
    for value in [
        "",
        "http://hooks.example.com",
        "https://u:p@hooks.example.com",
        "hooks.example.com",
        "https://a.com,,",
    ] {
        assert_eq!(
            WebhookUrlAllowlist::try_from(value),
            Err(ValueError::MalformedWebhookAllowlistEntry),
            "{value:?}"
        );
    }
}

#[test]
fn aliases_follow_postmarks_rule() {
    for valid in ["welcome", "w", "Welcome.v2_final-3"] {
        assert!(alias(valid.to_owned()).is_ok(), "{valid}");
    }
    for invalid in ["", "1welcome", "_welcome", "has space", "slash/es", "ünïcode"] {
        assert_eq!(
            alias(invalid.to_owned()),
            Err(ValueError::InvalidIdentifier { field: "alias" }),
            "{invalid:?}"
        );
    }
}

#[test]
fn a_new_webhook_enables_exactly_the_triggers_given() -> Result<(), ValueError> {
    let webhook = NewWebhook::try_from((url("https://h.example.com/")?, None, vec![WebhookTrigger::Click]))?;
    let body = serde_json::to_value(&webhook).unwrap_or_default();

    assert_eq!(body.pointer("/Triggers/Click/Enabled"), Some(&serde_json::json!(true)));
    assert_eq!(body.pointer("/Triggers/Open/Enabled"), Some(&serde_json::json!(false)));
    assert_eq!(body.get("MessageStream"), None);
    Ok(())
}
