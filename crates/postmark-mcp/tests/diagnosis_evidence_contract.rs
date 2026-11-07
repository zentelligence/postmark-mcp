//! Contract tests for how `diagnose_delivery` weighs evidence: which bounce records
//! count, which message and stream they belong to, and when what was read cannot
//! support a conclusion.

mod support;

use serde_json::{Value, json};
use support::diagnosis::*;
use support::{DEFAULT_STREAM, TestResult, assert_refused, success};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn a_message_id_postmark_rejects_is_not_read_as_no_recent_message() -> TestResult {
    let postmark = MockServer::start().await;
    Mock::given(path("/messages/outbound/m1/details"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({"ErrorCode": 701, "Message": "Not found"})))
        .mount(&postmark)
        .await;
    answer_empty_for(&postmark, DEFAULT_STREAM).await;

    let result = success(
        &postmark,
        "diagnose_delivery",
        json!({"recipient": RECIPIENT, "message_id": "m1"}),
    )
    .await?;

    assert_eq!(verdict(&result), Some("inconclusive"));
    assert_eq!(result.pointer("/incomplete/0/code"), Some(&json!("postmark/rejected")));
    Ok(())
}

#[tokio::test]
async fn out_of_office_replies_in_the_bounce_log_are_not_bounces() -> TestResult {
    for kind in ["AutoResponder", "SpamNotification", "Unknown", "VirusNotification"] {
        let delivered = diagnose(Scenario {
            messages: Some(sent_message()),
            details: Some(details("Sent", &json!([event("Delivered", RECIPIENT)]))),
            bounces: Some(json!([typed_bounce(5, kind, RECIPIENT, "m1", false)])),
            ..Scenario::default()
        })
        .await?;
        let no_events = diagnose(Scenario {
            messages: Some(sent_message()),
            details: Some(details("Sent", &json!([]))),
            bounces: Some(json!([typed_bounce(5, kind, RECIPIENT, "m1", false)])),
            ..Scenario::default()
        })
        .await?;

        assert_eq!(verdict(&delivered), Some("delivered"), "{kind}");
        assert_eq!(verdict(&no_events), Some("sent_no_delivery_event"), "{kind}");
    }
    Ok(())
}

#[tokio::test]
async fn a_bounced_event_for_a_non_failure_record_is_not_a_bounce() -> TestResult {
    let bounced = json!({"Type": "Bounced", "Recipient": RECIPIENT, "Details": {"BounceID": "5"}});

    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT), bounced]))),
        bounces: Some(json!([typed_bounce(5, "AutoResponder", RECIPIENT, "m1", false)])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("delivered"));
    Ok(())
}

#[tokio::test]
async fn temporary_records_in_the_bounce_log_mean_delayed() -> TestResult {
    for kind in ["Transient", "DnsError"] {
        let result = diagnose(Scenario {
            messages: Some(sent_message()),
            details: Some(details("Sent", &json!([]))),
            bounces: Some(json!([typed_bounce(5, kind, RECIPIENT, "m1", false)])),
            ..Scenario::default()
        })
        .await?;

        assert_eq!(verdict(&result), Some("delayed"), "{kind}");
    }
    Ok(())
}

#[tokio::test]
async fn a_bounce_of_an_older_message_is_not_this_messages_bounce() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT)]))),
        bounces: Some(json!([bounce(5, RECIPIENT, "older", false)])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("delivered"));
    Ok(())
}

#[tokio::test]
async fn delivery_after_a_temporary_failure_is_delivery() -> TestResult {
    for later in ["Delivered", "Opened", "LinkClicked"] {
        let result = diagnose(Scenario {
            messages: Some(sent_message()),
            details: Some(details(
                "Sent",
                &json!([event("Transient", RECIPIENT), event(later, RECIPIENT)]),
            )),
            ..Scenario::default()
        })
        .await?;

        assert_eq!(verdict(&result), Some("delivered"), "{later}");
    }
    Ok(())
}

#[tokio::test]
async fn sandboxed_messages_are_not_reported_as_awaiting_delivery() -> TestResult {
    let mut message = details("Sent", &json!([]));
    message["Sandboxed"] = json!(true);

    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(message),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("sandboxed"));
    Ok(())
}

#[tokio::test]
async fn a_given_message_is_checked_against_its_own_stream() -> TestResult {
    let mut message = details("Sent", &json!([]));
    message["MessageStream"] = json!("broadcast");
    let postmark = MockServer::start().await;
    Mock::given(path("/messages/outbound/m1/details"))
        .respond_with(ResponseTemplate::new(200).set_body_json(message))
        .mount(&postmark)
        .await;
    Mock::given(path("/message-streams/broadcast/suppressions/dump"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Suppressions": [suppression(RECIPIENT, "ManualSuppression", "Recipient")],
        })))
        .mount(&postmark)
        .await;
    Mock::given(path("/bounces"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"TotalCount": 0, "Bounces": []})))
        .mount(&postmark)
        .await;

    let result = success(
        &postmark,
        "diagnose_delivery",
        json!({"recipient": RECIPIENT, "message_id": "m1"}),
    )
    .await?;

    assert_eq!(verdict(&result), Some("suppressed_manual"));
    assert_eq!(result.get("message_stream"), Some(&json!("broadcast")));
    let requests = postmark.received_requests().await.unwrap_or_default();
    let bounce_lookups: Vec<_> = requests
        .iter()
        .filter(|request| request.url.path() == "/bounces")
        .collect();
    assert_eq!(bounce_lookups.len(), 2);
    for request in bounce_lookups {
        assert!(
            request
                .url
                .query_pairs()
                .any(|(name, value)| name == "messagestream" && value == "broadcast"),
            "bounce lookup left the message's stream: {}",
            request.url
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_event_with_no_recipient_counts_only_on_a_single_recipient_message() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([{"Type": "Delivered"}]))),
        ..Scenario::default()
    })
    .await?;
    assert_eq!(verdict(&result), Some("delivered"));

    let mut shared = details("Sent", &json!([{"Type": "Delivered"}]));
    shared["Recipients"] = json!([RECIPIENT, "bob@example.com"]);
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(shared),
        ..Scenario::default()
    })
    .await?;
    assert_eq!(verdict(&result), Some("sent_no_delivery_event"));
    Ok(())
}

#[tokio::test]
async fn a_search_hit_with_no_recipients_is_unconfirmed_rather_than_adopted_or_dropped() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(json!([{"MessageID": "m1", "Recipients": null, "Status": "Sent"}])),
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT)]))),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("inconclusive"));
    assert_eq!(
        result.pointer("/incomplete/0/code"),
        Some(&json!("diagnosis/recipient-unconfirmed"))
    );
    Ok(())
}

#[tokio::test]
async fn a_given_message_with_no_recipients_is_unconfirmed() -> TestResult {
    let mut message = details("Sent", &json!([event("Delivered", "bob@example.com")]));
    message["Recipients"] = json!([]);
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
        Some(&json!("diagnosis/recipient-unconfirmed"))
    );
    Ok(())
}

#[tokio::test]
async fn a_recipient_listed_with_a_display_name_still_matches() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(json!([{"MessageID": "m1", "Recipients": ["Alice <alice@example.com>"], "Status": "Sent"}])),
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT)]))),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("delivered"));
    Ok(())
}

#[tokio::test]
async fn the_first_exact_match_is_the_message_investigated() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(json!([
            {"MessageID": "other", "Recipients": ["malice@example.com"]},
            {"MessageID": "m1", "Recipients": [RECIPIENT]},
            {"MessageID": "m2", "Recipients": [RECIPIENT]},
        ])),
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT)]))),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("delivered"));
    Ok(())
}

#[tokio::test]
async fn results_cut_off_by_the_page_size_are_not_read_as_absence() -> TestResult {
    let hits: Vec<_> = (0..10)
        .map(|index| json!({"MessageID": format!("x{index}"), "Recipients": ["malice@example.com"]}))
        .collect();
    let postmark = MockServer::start().await;
    Mock::given(path("/messages/outbound"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"TotalCount": 11, "Messages": hits})))
        .mount(&postmark)
        .await;
    answer_empty_for(&postmark, DEFAULT_STREAM).await;

    let result = success(&postmark, "diagnose_delivery", json!({"recipient": RECIPIENT})).await?;

    assert_eq!(verdict(&result), Some("inconclusive"));
    assert_eq!(
        result.pointer("/incomplete/0/code"),
        Some(&json!("diagnosis/results-truncated"))
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_bounce_lookup_blocks_conclusions_that_need_it() -> TestResult {
    let no_message = diagnose(Scenario {
        bounces: None,
        ..Scenario::default()
    })
    .await?;
    let no_events = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([]))),
        bounces: None,
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&no_message), Some("inconclusive"));
    assert_eq!(incomplete_lookups(&no_message), ["bounces"]);
    assert_eq!(verdict(&no_events), Some("inconclusive"));
    assert_eq!(incomplete_lookups(&no_events), ["bounces", "message_bounces"]);
    Ok(())
}

#[tokio::test]
async fn the_bounce_id_to_reactivate_is_offered_only_for_a_hard_bounce_suppression() -> TestResult {
    let deactivated = typed_bounce(5, "HardBounce", RECIPIENT, "m0", true);
    for (reason, origin) in [("SpamComplaint", "Recipient"), ("ManualSuppression", "Recipient")] {
        let result = diagnose(Scenario {
            suppressions: Some(json!([suppression(RECIPIENT, reason, origin)])),
            bounces: Some(json!([deactivated.clone()])),
            ..Scenario::default()
        })
        .await?;

        assert_eq!(result.get("reactivatable_bounce_id"), None, "{reason}");
    }

    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT)]))),
        bounces: Some(json!([deactivated])),
        ..Scenario::default()
    })
    .await?;
    assert_eq!(result.get("reactivatable_bounce_id"), None);
    Ok(())
}

#[tokio::test]
async fn the_bounce_offered_is_the_one_that_deactivated_the_address() -> TestResult {
    let mut soft = typed_bounce(200, "SoftBounce", RECIPIENT, "m2", true);
    soft["Inactive"] = json!(false);

    let result = diagnose(Scenario {
        suppressions: Some(json!([suppression(RECIPIENT, "HardBounce", "Recipient")])),
        bounces: Some(json!([soft, typed_bounce(100, "HardBounce", RECIPIENT, "m1", true)])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(result.get("reactivatable_bounce_id"), Some(&json!(100)));
    Ok(())
}

#[tokio::test]
async fn a_failure_on_record_with_no_message_found_is_not_no_recent_message() -> TestResult {
    let failed = diagnose(Scenario {
        bounces: Some(json!([bounce(7, RECIPIENT, "gone", false)])),
        ..Scenario::default()
    })
    .await?;
    let autoresponder = diagnose(Scenario {
        bounces: Some(json!([typed_bounce(7, "AutoResponder", RECIPIENT, "gone", false)])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&failed), Some("bounce_history_only"));
    assert_eq!(verdict(&autoresponder), Some("bounce_history_only"));
    Ok(())
}

#[tokio::test]
async fn an_unrecognised_suppression_reason_is_not_reported_as_manual() -> TestResult {
    let result = diagnose(Scenario {
        suppressions: Some(json!([suppression(RECIPIENT, "SomethingNew", "Admin")])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("suppressed_other"));
    Ok(())
}

#[tokio::test]
async fn each_incomplete_lookup_says_what_to_do_about_it() -> TestResult {
    let result = diagnose(Scenario {
        suppressions: None,
        ..Scenario::default()
    })
    .await?;

    assert!(
        result
            .pointer("/incomplete/0/remediation")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.is_empty())
    );
    Ok(())
}

#[tokio::test]
async fn bounces_of_the_message_itself_are_looked_up_directly() -> TestResult {
    let postmark = start(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([]))),
        ..Scenario::default()
    })
    .await;

    success(&postmark, "diagnose_delivery", json!({"recipient": RECIPIENT})).await?;

    let requests = postmark.received_requests().await.unwrap_or_default();
    assert!(
        requests.iter().any(|request| request.url.path() == "/bounces"
            && request
                .url
                .query_pairs()
                .any(|(name, value)| name == "messageID" && value == "m1")),
        "no bounce lookup for the message"
    );
    Ok(())
}

#[tokio::test]
async fn each_lookup_covers_the_stream_and_dates_requested() -> TestResult {
    let postmark = MockServer::start().await;
    Mock::given(path("/messages/outbound"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"TotalCount": 0, "Messages": []})))
        .mount(&postmark)
        .await;
    Mock::given(path("/message-streams/broadcast/suppressions/dump"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Suppressions": []})))
        .mount(&postmark)
        .await;
    Mock::given(path("/bounces"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"TotalCount": 0, "Bounces": []})))
        .mount(&postmark)
        .await;

    success(
        &postmark,
        "diagnose_delivery",
        json!({
            "recipient": RECIPIENT,
            "message_stream": "broadcast",
            "from_date": "2026-01-01",
            "to_date": "2026-01-31",
        }),
    )
    .await?;

    let requests = postmark.received_requests().await.unwrap_or_default();
    assert_eq!(
        query_of(&requests, "/messages/outbound")?,
        owned(&[
            ("count", "10"),
            ("offset", "0"),
            ("recipient", RECIPIENT),
            ("messagestream", "broadcast"),
            ("fromdate", "2026-01-01"),
            ("todate", "2026-01-31"),
        ])
    );
    assert_eq!(
        query_of(&requests, "/bounces")?,
        owned(&[
            ("count", "50"),
            ("offset", "0"),
            ("emailFilter", RECIPIENT),
            ("messagestream", "broadcast"),
            ("fromdate", "2026-01-01"),
            ("todate", "2026-01-31"),
        ])
    );
    // A suppression applies whenever it was created, so no dates here.
    assert_eq!(
        query_of(&requests, "/message-streams/broadcast/suppressions/dump")?,
        owned(&[("EmailAddress", RECIPIENT)])
    );
    Ok(())
}

#[tokio::test]
async fn with_no_dates_the_whole_retained_history_is_searched() -> TestResult {
    let postmark = start(Scenario::default()).await;

    success(&postmark, "diagnose_delivery", json!({"recipient": RECIPIENT})).await?;

    let requests = postmark.received_requests().await.unwrap_or_default();
    let search = query_of(&requests, "/messages/outbound")?;
    assert!(!search.iter().any(|(name, _)| name == "fromdate" || name == "todate"));
    Ok(())
}

#[tokio::test]
async fn bounces_returned_are_the_first_ten_postmark_lists() -> TestResult {
    let bounces: Vec<_> = (0..15).map(|id| bounce(id, RECIPIENT, "m0", false)).collect();

    let result = diagnose(Scenario {
        bounces: Some(json!(bounces)),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(
        result.get("recent_bounces").and_then(Value::as_array).map(Vec::len),
        Some(10)
    );
    assert_eq!(result.pointer("/recent_bounces/0/bounce_id"), Some(&json!(0)));
    Ok(())
}

#[tokio::test]
async fn recommendations_never_name_a_tool_this_server_lacks() -> TestResult {
    let scenarios = [
        Scenario::default(),
        Scenario {
            suppressions: Some(json!([suppression(RECIPIENT, "HardBounce", "Recipient")])),
            bounces: Some(json!([bounce(99, RECIPIENT, "m1", true)])),
            ..Scenario::default()
        },
        Scenario {
            suppressions: Some(json!([suppression(RECIPIENT, "ManualSuppression", "Customer")])),
            ..Scenario::default()
        },
    ];
    for scenario in scenarios {
        let result = diagnose(scenario).await?;
        let text = result.get("recommendation").and_then(Value::as_str).unwrap_or_default();

        assert_ne!(text, "");
        for missing in [
            "send_email",
            "activate_bounce",
            "delete_suppressions",
            "sendEmail",
            "activateBounce",
        ] {
            assert!(!text.contains(missing), "{text}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn invalid_arguments_are_refused() -> TestResult {
    for arguments in [
        json!({}),
        json!({"recipient": "not-an-address"}),
        json!({"recipient": "two@at@example.com"}),
        json!({"recipient": "Alice <alice@example.com>"}),
        json!({"recipient": "alice@example.com\nbcc@example.com"}),
        json!({"recipient": RECIPIENT, "message_id": ".."}),
        json!({"recipient": RECIPIENT, "message_stream": "a/b"}),
        json!({"recipient": RECIPIENT, "from_date": "2026-02-30"}),
        json!({"recipient": RECIPIENT, "from_date": "2026-02-01", "to_date": "2026-01-01"}),
        json!({"recipient": RECIPIENT, "unexpected": true}),
        json!({"recipient": RECIPIENT, "message_id": "m1", "from_date": "2026-01-01"}),
        json!({"recipient": RECIPIENT, "message_id": "m1", "to_date": "2026-01-01"}),
    ] {
        assert_refused("diagnose_delivery", arguments).await?;
    }
    Ok(())
}

#[tokio::test]
async fn soft_bounces_are_failures_because_nothing_says_they_are_retried() -> TestResult {
    let record = typed_bounce(9, "SoftBounce", RECIPIENT, "m1", false);
    let linked_event = json!({"Type": "Bounced", "Recipient": RECIPIENT, "Details": {"BounceID": "9"}});

    let from_record = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([]))),
        bounces: Some(json!([record.clone()])),
        ..Scenario::default()
    })
    .await?;
    let from_event = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([linked_event]))),
        bounces: Some(json!([record])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&from_record), Some("bounced"));
    assert_eq!(verdict(&from_event), Some("bounced"));
    Ok(())
}

#[tokio::test]
async fn a_bounced_event_stands_unless_its_record_says_otherwise() -> TestResult {
    for kind in ["Unknown", "ManuallyDeactivated"] {
        let linked_event = json!({"Type": "Bounced", "Recipient": RECIPIENT, "Details": {"BounceID": 9}});

        let result = diagnose(Scenario {
            messages: Some(sent_message()),
            details: Some(details("Sent", &json!([linked_event]))),
            bounces: Some(json!([typed_bounce(9, kind, RECIPIENT, "m1", false)])),
            ..Scenario::default()
        })
        .await?;

        assert_eq!(verdict(&result), Some("bounced"), "{kind}");
    }
    Ok(())
}

#[tokio::test]
async fn a_numeric_bounce_id_links_an_event_to_its_record() -> TestResult {
    let linked_event = json!({"Type": "Bounced", "Recipient": RECIPIENT, "Details": {"BounceID": 5}});

    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT), linked_event]))),
        bounces: Some(json!([typed_bounce(5, "AutoResponder", RECIPIENT, "m1", false)])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("delivered"));
    Ok(())
}

#[tokio::test]
async fn a_bounced_event_is_linked_to_a_record_from_either_lookup() -> TestResult {
    let linked_event = json!({"Type": "Bounced", "Recipient": RECIPIENT, "Details": {"BounceID": "5"}});

    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT), linked_event]))),
        bounces: Some(json!([typed_bounce(5, "AutoResponder", RECIPIENT, "other", false)])),
        message_bounces: MessageBounces::Answer(json!([])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("delivered"));
    Ok(())
}

#[tokio::test]
async fn a_bounce_found_only_by_message_id_is_used_and_shown() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([]))),
        bounces: Some(json!([])),
        message_bounces: MessageBounces::Answer(json!([bounce(5, RECIPIENT, "m1", false)])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("bounced"));
    assert_eq!(result.pointer("/recent_bounces/0/bounce_id"), Some(&json!(5)));
    Ok(())
}

#[tokio::test]
async fn a_record_found_by_both_lookups_appears_once() -> TestResult {
    let record = bounce(5, RECIPIENT, "m1", false);

    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([]))),
        bounces: Some(json!([record.clone()])),
        message_bounces: MessageBounces::Answer(json!([record])),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(
        result.get("recent_bounces").and_then(Value::as_array).map(Vec::len),
        Some(1)
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_message_bounce_lookup_alone_blocks_conclusions_from_absence() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([]))),
        message_bounces: MessageBounces::Fails,
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("inconclusive"));
    assert_eq!(incomplete_lookups(&result), ["message_bounces"]);
    Ok(())
}

#[tokio::test]
async fn a_failure_in_the_history_counts_when_the_message_lookup_fails() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([event("Delivered", RECIPIENT)]))),
        bounces: Some(json!([bounce(5, RECIPIENT, "m1", false)])),
        message_bounces: MessageBounces::Fails,
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("bounced"));
    Ok(())
}

#[tokio::test]
async fn sandboxing_outranks_any_recorded_event() -> TestResult {
    let mut message = details("Sent", &json!([event("Bounced", RECIPIENT)]));
    message["Sandboxed"] = json!(true);

    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(message),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(verdict(&result), Some("sandboxed"));
    Ok(())
}

#[tokio::test]
async fn delayed_and_queued_need_the_bounce_lookups_that_could_refute_them() -> TestResult {
    for message in [
        details("Sent", &json!([event("Transient", RECIPIENT)])),
        details("Queued", &json!([])),
    ] {
        let result = diagnose(Scenario {
            messages: Some(sent_message()),
            details: Some(message),
            bounces: None,
            ..Scenario::default()
        })
        .await?;

        assert_eq!(verdict(&result), Some("inconclusive"));
    }
    Ok(())
}

#[tokio::test]
async fn a_given_message_with_no_usable_stream_is_flagged() -> TestResult {
    for stream in [Value::Null, json!("bad/stream")] {
        let mut message = details("Sent", &json!([]));
        message["MessageStream"] = stream;
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
            Some(&json!("diagnosis/stream-unconfirmed"))
        );
    }
    Ok(())
}

#[tokio::test]
async fn delivery_stands_when_a_given_messages_stream_is_unknown() -> TestResult {
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
    assert_eq!(
        result.pointer("/incomplete/0/code"),
        Some(&json!("diagnosis/stream-unconfirmed"))
    );
    Ok(())
}

#[tokio::test]
async fn a_search_that_found_a_message_needs_no_stream_on_it() -> TestResult {
    let result = diagnose(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([]))),
        ..Scenario::default()
    })
    .await?;

    assert_eq!(result.get("incomplete"), Some(&json!([])));
    Ok(())
}

#[tokio::test]
async fn a_full_page_with_nothing_beyond_it_is_not_truncated() -> TestResult {
    let hits: Vec<_> = (0..10)
        .map(|index| json!({"MessageID": format!("x{index}"), "Recipients": ["malice@example.com"]}))
        .collect();
    let postmark = MockServer::start().await;
    Mock::given(path("/messages/outbound"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"TotalCount": 10, "Messages": hits})))
        .mount(&postmark)
        .await;
    answer_empty_for(&postmark, DEFAULT_STREAM).await;

    let result = success(&postmark, "diagnose_delivery", json!({"recipient": RECIPIENT})).await?;

    assert_eq!(verdict(&result), Some("no_recent_message"));
    Ok(())
}

#[tokio::test]
async fn a_bounce_page_cut_off_by_other_addresses_is_not_read_as_absence() -> TestResult {
    let others: Vec<_> = (0..50).map(|id| bounce(id, "malice@example.com", "x", false)).collect();
    let postmark = MockServer::start().await;
    Mock::given(path("/messages/outbound"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"TotalCount": 0, "Messages": []})))
        .mount(&postmark)
        .await;
    Mock::given(path(format!("/message-streams/{DEFAULT_STREAM}/suppressions/dump")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Suppressions": []})))
        .mount(&postmark)
        .await;
    Mock::given(path("/bounces"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"TotalCount": 51, "Bounces": others})))
        .mount(&postmark)
        .await;

    let result = success(&postmark, "diagnose_delivery", json!({"recipient": RECIPIENT})).await?;

    assert_eq!(verdict(&result), Some("inconclusive"));
    assert_eq!(result.pointer("/incomplete/0/lookup"), Some(&json!("bounces")));
    assert_eq!(
        result.pointer("/incomplete/0/code"),
        Some(&json!("diagnosis/results-truncated"))
    );
    Ok(())
}

#[tokio::test]
async fn the_message_bounce_lookup_is_not_limited_by_dates() -> TestResult {
    let postmark = start(Scenario {
        messages: Some(sent_message()),
        details: Some(details("Sent", &json!([]))),
        ..Scenario::default()
    })
    .await;

    success(
        &postmark,
        "diagnose_delivery",
        json!({"recipient": RECIPIENT, "from_date": "2026-01-01", "to_date": "2026-01-31"}),
    )
    .await?;

    let requests = postmark.received_requests().await.unwrap_or_default();
    let lookup = requests
        .iter()
        .find(|request| request.url.query_pairs().any(|(name, _)| name == "messageID"))
        .ok_or("no bounce lookup by message ID")?;
    assert!(
        !lookup
            .url
            .query_pairs()
            .any(|(name, _)| name == "fromdate" || name == "todate")
    );
    Ok(())
}
