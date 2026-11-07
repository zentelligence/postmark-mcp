//! Contract tests for `diagnose_delivery`: verdicts, failed lookups, and how the
//! arguments reach each lookup. The recurring concern is a confident wrong answer:
//! a failed lookup, or another address's data, must never read as a finding about
//! this recipient.

mod support;

use serde_json::json;
use support::diagnosis::*;
use support::{DEFAULT_STREAM, TestResult, failure, success};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn a_delivered_message_is_reported_with_its_events() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT)]))),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("delivered"));
    assert_eq!(result.get("recipient"), Some(&json!(RECIPIENT)));
    assert_eq!(result.get("message_stream"), Some(&json!(DEFAULT_STREAM)));
    assert_eq!(result.pointer("/investigated_message/message_id"), Some(&json!("m1")));
    assert_eq!(
        result.pointer("/investigated_message/events/0/type"),
        Some(&json!("Delivered"))
    );
    assert_eq!(result.get("incomplete"), Some(&json!([])));
    Ok(())
}

#[tokio::test]
async fn delivery_to_another_recipient_of_the_same_message_does_not_count() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([event("Delivered", "bob@example.com")]))),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("sent_no_delivery_event"));
    assert_eq!(result.pointer("/investigated_message/events"), Some(&json!([])));
    Ok(())
}

#[tokio::test]
async fn another_addresss_suppression_is_not_this_recipients() -> TestResult {
    let result = diagnose(Scenario {
        suppressions: Some(json!([suppression("malice@example.com", "SpamComplaint", "Recipient")])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("no_recent_message"));
    assert_eq!(result.get("suppression"), None);
    Ok(())
}

#[tokio::test]
async fn another_addresss_bounces_are_not_this_recipients() -> TestResult {
    let result = diagnose(Scenario {
        bounces: Some(json!([bounce(1, "malice@example.com", "m9", true)])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(result.get("recent_bounces"), Some(&json!([])));
    Ok(())
}

#[tokio::test]
async fn a_search_hit_for_another_address_is_not_this_recipients_message() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(json!([{"MessageID": "m1", "Recipients": ["malice@example.com"], "Status": "Sent"}])),
        details: Some(details("Sent", &json!([event("Delivered", "malice@example.com")]))),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("no_recent_message"));
    assert_eq!(result.get("investigated_message"), None);
    Ok(())
}

#[tokio::test]
async fn addresses_match_regardless_of_case() -> TestResult {
    let result = diagnose(Scenario {
        suppressions: Some(json!([suppression("Alice@Example.COM", "HardBounce", "Recipient")])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("suppressed_hard_bounce"));
    Ok(())
}

#[tokio::test]
async fn a_spam_complaint_is_final() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT)]))),
        suppressions: Some(json!([suppression(RECIPIENT, "SpamComplaint", "Recipient")])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("suppressed_spam_complaint"));
    assert_eq!(
        result.pointer("/suppression/suppression_reason"),
        Some(&json!("SpamComplaint"))
    );
    Ok(())
}

#[tokio::test]
async fn a_hard_bounce_names_the_bounce_that_can_be_reactivated() -> TestResult {
    let result = diagnose(Scenario {
        suppressions: Some(json!([suppression(RECIPIENT, "HardBounce", "Recipient")])),
        bounces: Some(json!([
            bounce(98, RECIPIENT, "m0", false),
            bounce(99, RECIPIENT, "m1", true)
        ])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("suppressed_hard_bounce"));
    assert_eq!(result.get("reactivatable_bounce_id"), Some(&json!(99)));
    Ok(())
}

#[tokio::test]
async fn a_hard_bounce_that_cannot_be_reactivated_names_no_bounce() -> TestResult {
    let result = diagnose(Scenario {
        suppressions: Some(json!([suppression(RECIPIENT, "HardBounce", "Recipient")])),
        bounces: Some(json!([bounce(98, RECIPIENT, "m0", false)])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(result.get("reactivatable_bounce_id"), None);
    Ok(())
}

#[tokio::test]
async fn a_recipients_own_opt_out_is_not_presented_as_removable() -> TestResult {
    let opted_out = diagnose(Scenario {
        suppressions: Some(json!([suppression(RECIPIENT, "ManualSuppression", "Recipient")])),
        ..Scenario::default()
    })
    .await?;
    let by_customer = diagnose(Scenario {
        suppressions: Some(json!([suppression(RECIPIENT, "ManualSuppression", "Customer")])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&opted_out), Some("suppressed_manual"));
    assert_eq!(verdict(&by_customer), Some("suppressed_manual"));
    assert_ne!(opted_out.get("recommendation"), by_customer.get("recommendation"));
    Ok(())
}

#[tokio::test]
async fn a_bounced_message_is_identified_from_its_events() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details(
            "Sent",
            &json!([event("Delivered", RECIPIENT), event("Bounced", RECIPIENT)]),
        )),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("bounced"));
    Ok(())
}

#[tokio::test]
async fn a_bounced_message_is_identified_from_the_bounce_log() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([]))),
        bounces: Some(json!([bounce(99, RECIPIENT, "m1", true)])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("bounced"));
    Ok(())
}

#[tokio::test]
async fn a_transient_failure_is_reported_as_delayed() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([event("Transient", RECIPIENT)]))),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("delayed"));
    Ok(())
}

#[tokio::test]
async fn a_queued_message_is_reported_as_queued() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Queued", &json!([]))),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("queued"));
    Ok(())
}

#[tokio::test]
async fn a_failed_suppression_check_never_reads_as_not_suppressed() -> TestResult {
    let result = diagnose(Scenario {
        suppressions: None,
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("inconclusive"));
    assert_eq!(incomplete_lookups(&result), ["suppressions"]);
    assert_eq!(
        result.pointer("/incomplete/0/code"),
        Some(&json!("postmark/unavailable"))
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_suppression_check_leaves_an_undelivered_message_inconclusive() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([]))),
        suppressions: None,
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("inconclusive"));
    Ok(())
}

#[tokio::test]
async fn delivery_is_still_reported_when_the_suppression_check_fails() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT)]))),
        suppressions: None,
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("delivered"));
    assert_eq!(incomplete_lookups(&result), ["suppressions"]);
    Ok(())
}

#[tokio::test]
async fn a_failed_message_search_never_reads_as_no_recent_message() -> TestResult {
    let result = diagnose(Scenario {
        messages: None,
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("inconclusive"));
    assert_eq!(incomplete_lookups(&result), ["message"]);
    Ok(())
}

#[tokio::test]
async fn a_found_suppression_stands_when_other_lookups_fail() -> TestResult {
    let result = diagnose(Scenario {
        messages: None,
        suppressions: Some(json!([suppression(RECIPIENT, "HardBounce", "Recipient")])),
        bounces: None,
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("suppressed_hard_bounce"));
    assert_eq!(incomplete_lookups(&result), ["message", "bounces"]);
    Ok(())
}

#[tokio::test]
async fn failed_details_keep_what_the_search_found() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: None,
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("inconclusive"));
    assert_eq!(result.pointer("/investigated_message/message_id"), Some(&json!("m1")));
    assert_eq!(result.pointer("/investigated_message/events"), None);
    assert_eq!(incomplete_lookups(&result), ["message_details"]);
    Ok(())
}

#[tokio::test]
async fn when_every_lookup_fails_the_tool_fails_rather_than_diagnosing() -> TestResult {
    let postmark = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"ErrorCode": 10, "Message": "Bad token"})))
        .mount(&postmark)
        .await;

    let result = failure(&postmark, "diagnose_delivery", json!({"recipient": RECIPIENT})).await?;

    assert_eq!(result.get("code"), Some(&json!("postmark/unauthorised")));
    Ok(())
}

#[tokio::test]
async fn a_given_message_id_is_read_directly() -> TestResult {
    let postmark = start(Scenario {
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT)]))),
        ..Scenario::default()
    })
    .await;

    let result = success(
        &postmark,
        "diagnose_delivery",
        json!({"recipient": RECIPIENT, "message_id": "m1"}),
    )
    .await?;

    assert_eq!(verdict(&result), Some("delivered"));
    let requests = postmark.received_requests().await.unwrap_or_default();
    assert!(
        !requests
            .iter()
            .any(|request| request.url.path() == "/messages/outbound"),
        "searched despite a given message ID"
    );
    Ok(())
}

#[tokio::test]
async fn a_message_id_for_another_recipient_is_flagged_rather_than_diagnosed() -> TestResult {
    let mut message = details("Sent", &json!([event("Delivered", "bob@example.com")]));
    message["Recipients"] = json!(["bob@example.com"]);
    let postmark = start(Scenario {
        details: Some(message),
        ..Scenario::default()
    })
    .await;

    let result = success(
        &postmark,
        "diagnose_delivery",
        json!({"recipient": RECIPIENT, "message_id": "m1"}),
    )
    .await?;

    assert_eq!(verdict(&result), Some("inconclusive"));
    assert_eq!(
        result.pointer("/incomplete/0/code"),
        Some(&json!("diagnosis/recipient-mismatch"))
    );
    assert_eq!(
        result.get("investigated_message"),
        None,
        "reported another recipient's message"
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_message_id_is_not_read_as_no_recent_message() -> TestResult {
    let postmark = start(Scenario::default()).await;

    let result = success(
        &postmark,
        "diagnose_delivery",
        json!({"recipient": RECIPIENT, "message_id": "m1"}),
    )
    .await?;

    assert_eq!(verdict(&result), Some("inconclusive"));
    assert_eq!(incomplete_lookups(&result), ["message"]);
    Ok(())
}
