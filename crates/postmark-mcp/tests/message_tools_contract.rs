//! Contract tests for `search_outbound_messages` and `get_message_details`.

mod support;

use serde_json::json;
use support::{DEFAULT_STREAM, TestResult, assert_refused, postmark_answering, query, received_query, success};
use wiremock::MockServer;

async fn empty_search() -> MockServer {
    postmark_answering("GET", "/messages/outbound", json!({"TotalCount": 0, "Messages": []})).await
}

#[tokio::test]
async fn search_defaults_to_fifty_results_on_the_configured_stream() -> TestResult {
    let postmark = empty_search().await;

    success(&postmark, "search_outbound_messages", json!({})).await?;

    assert_eq!(
        received_query(&postmark).await?,
        query(&[("count", "50"), ("offset", "0"), ("messagestream", DEFAULT_STREAM)])
    );
    Ok(())
}

#[tokio::test]
async fn search_sends_every_filter_under_postmarks_parameter_name() -> TestResult {
    let postmark = empty_search().await;

    success(
        &postmark,
        "search_outbound_messages",
        json!({
            "recipient": "to+tag@example.com",
            "from_email": "from@example.com",
            "tag": "welcome",
            "subject": "Hello there",
            "status": "sent",
            "message_stream": "broadcast",
            "from_date": "2026-01-01",
            "to_date": "2026-01-31",
            "count": 10,
            "offset": 20,
        }),
    )
    .await?;

    assert_eq!(
        received_query(&postmark).await?,
        query(&[
            ("recipient", "to+tag@example.com"),
            ("fromemail", "from@example.com"),
            ("tag", "welcome"),
            ("subject", "Hello there"),
            ("status", "sent"),
            ("messagestream", "broadcast"),
            ("fromdate", "2026-01-01"),
            ("todate", "2026-01-31"),
            ("count", "10"),
            ("offset", "20"),
        ])
    );
    Ok(())
}

#[tokio::test]
async fn search_returns_message_summaries() -> TestResult {
    let postmark = postmark_answering(
        "GET",
        "/messages/outbound",
        json!({
            "TotalCount": 120,
            "Messages": [{
                "Tag": "welcome",
                "MessageID": "0ac29aee",
                "MessageStream": "outbound",
                "To": [{"Email": "to@example.com", "Name": null}],
                "Recipients": ["to@example.com"],
                "ReceivedAt": "2026-01-02T03:04:05-05:00",
                "From": "Sender <from@example.com>",
                "Subject": "Hello",
                "Attachments": [],
                "Status": "Sent",
                "TrackOpens": true,
                "TrackLinks": "HtmlAndText",
                "Metadata": {"color": "blue"},
                "Sandboxed": false,
            }],
        }),
    )
    .await;

    let result = success(&postmark, "search_outbound_messages", json!({})).await?;

    assert_eq!(
        result,
        json!({
            "total_count": 120,
            "messages": [{
                "message_id": "0ac29aee",
                "message_stream": "outbound",
                "recipients": ["to@example.com"],
                "received_at": "2026-01-02T03:04:05-05:00",
                "from": "Sender <from@example.com>",
                "subject": "Hello",
                "status": "Sent",
                "tag": "welcome",
                "metadata": {"color": "blue"},
            }],
        })
    );
    Ok(())
}

#[tokio::test]
async fn optional_objects_in_an_unexpected_shape_do_not_fail_the_search() -> TestResult {
    // Postmark documents `Metadata` as an array but shows an object.
    let postmark = postmark_answering(
        "GET",
        "/messages/outbound",
        json!({"TotalCount": 1, "Messages": [{"MessageID": "abc", "Metadata": []}]}),
    )
    .await;

    let result = success(&postmark, "search_outbound_messages", json!({})).await?;

    assert_eq!(result.pointer("/messages/0/message_id"), Some(&json!("abc")));
    assert_eq!(result.pointer("/messages/0/metadata"), None);
    Ok(())
}

#[tokio::test]
async fn search_refuses_invalid_filters() -> TestResult {
    for arguments in [
        json!({"count": 0}),
        json!({"count": 501}),
        json!({"count": 500, "offset": 9501}),
        json!({"from_date": "2026-13-01"}),
        json!({"from_date": "2026-02-30"}),
        json!({"to_date": "01/02/2026"}),
        json!({"from_date": "2026-02-01", "to_date": "2026-01-01"}),
        json!({"status": "bounced"}),
        json!({"message_stream": "../servers"}),
        json!({"tag": ""}),
        json!({"recipient": "line\nbreak"}),
        json!({"unexpected": true}),
    ] {
        assert_refused("search_outbound_messages", arguments).await?;
    }
    Ok(())
}

#[tokio::test]
async fn search_allows_the_largest_page_postmark_permits() -> TestResult {
    let postmark = empty_search().await;

    success(
        &postmark,
        "search_outbound_messages",
        json!({"count": 500, "offset": 9500}),
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn details_returns_the_event_timeline_without_message_bodies() -> TestResult {
    let postmark = postmark_answering(
        "GET",
        "/messages/outbound/0ac29aee/details",
        json!({
            "TextBody": "secret text body",
            "HtmlBody": "secret html body",
            "Body": "secret raw source",
            "Tag": "welcome",
            "MessageID": "0ac29aee",
            "MessageStream": "outbound",
            "Recipients": ["to@example.com"],
            "ReceivedAt": "2026-01-02T03:04:05-05:00",
            "From": "from@example.com",
            "Subject": "Hello",
            "Status": "Sent",
            "TrackOpens": true,
            "TrackLinks": "None",
            "Metadata": {},
            "Sandboxed": false,
            "MessageEvents": [
                {
                    "Recipient": "to@example.com",
                    "Type": "Delivered",
                    "ReceivedAt": "2026-01-02T03:04:06-05:00",
                    "Details": {"DeliveryMessage": "250 OK", "DestinationServer": "mx.example.com", "DestinationIP": "192.0.2.1"},
                },
                {
                    "Recipient": "to@example.com",
                    "Type": "Bounced",
                    "ReceivedAt": "2026-01-02T03:04:07-05:00",
                    "Details": {"Summary": "Mailbox full", "BounceID": "99"},
                },
            ],
        }),
    )
    .await;

    let result = success(&postmark, "get_message_details", json!({"message_id": "0ac29aee"})).await?;

    assert_eq!(
        result,
        json!({
            "message_id": "0ac29aee",
            "message_stream": "outbound",
            "recipients": ["to@example.com"],
            "received_at": "2026-01-02T03:04:05-05:00",
            "from": "from@example.com",
            "subject": "Hello",
            "status": "Sent",
            "tag": "welcome",
            "track_opens": true,
            "track_links": "None",
            "sandboxed": false,
            "metadata": {},
            "events": [
                {
                    "recipient": "to@example.com",
                    "type": "Delivered",
                    "received_at": "2026-01-02T03:04:06-05:00",
                    "details": {"DeliveryMessage": "250 OK", "DestinationServer": "mx.example.com", "DestinationIP": "192.0.2.1"},
                },
                {
                    "recipient": "to@example.com",
                    "type": "Bounced",
                    "received_at": "2026-01-02T03:04:07-05:00",
                    "details": {"Summary": "Mailbox full", "BounceID": "99"},
                },
            ],
        })
    );
    Ok(())
}

#[tokio::test]
async fn details_never_relay_link_urls_or_unrecognised_event_data() -> TestResult {
    let postmark = postmark_answering(
        "GET",
        "/messages/outbound/abc/details",
        json!({
            "MessageID": "abc",
            "MessageEvents": [
                {
                    "Recipient": "to@example.com",
                    "Type": "LinkClicked",
                    "ReceivedAt": "2026-01-02T03:04:05-05:00",
                    "Details": {
                        "Link": "https://u:linkpass@app.example.com/reset?token=resettoken",
                        "ClickLocation": "HTML",
                        "Summary": "Clicked https://app.example.com/reset?token=resettoken",
                    },
                },
                {
                    "Type": "Opened",
                    "Details": {"Summary": "Opened in Mail", "Geo": {"City": "Hobart"}, "IP": "192.0.2.9"},
                },
            ],
        }),
    )
    .await;

    let result = success(&postmark, "get_message_details", json!({"message_id": "abc"})).await?;

    assert_eq!(
        result.pointer("/events/0/details"),
        Some(&json!({"ClickLocation": "HTML"}))
    );
    assert_eq!(
        result.pointer("/events/1/details"),
        Some(&json!({"Summary": "Opened in Mail"}))
    );
    let serialised = result.to_string();
    for secret in ["linkpass", "resettoken", "Hobart", "192.0.2.9"] {
        assert!(!serialised.contains(secret), "{secret} reached the tool result");
    }
    Ok(())
}

#[tokio::test]
async fn details_tolerate_optional_objects_in_an_unexpected_shape() -> TestResult {
    let postmark = postmark_answering(
        "GET",
        "/messages/outbound/abc/details",
        json!({"MessageID": "abc", "Metadata": [], "MessageEvents": [{"Type": "Opened", "Details": []}]}),
    )
    .await;

    let result = success(&postmark, "get_message_details", json!({"message_id": "abc"})).await?;

    assert_eq!(result.get("metadata"), None);
    assert_eq!(result.pointer("/events/0/type"), Some(&json!("Opened")));
    Ok(())
}

#[tokio::test]
async fn details_reports_no_events_as_an_empty_timeline() -> TestResult {
    let postmark = postmark_answering(
        "GET",
        "/messages/outbound/abc/details",
        json!({"MessageID": "abc", "MessageEvents": null}),
    )
    .await;

    let result = success(&postmark, "get_message_details", json!({"message_id": "abc"})).await?;

    assert_eq!(result.get("events"), Some(&json!([])));
    Ok(())
}

#[tokio::test]
async fn details_refuses_ids_that_would_not_name_a_message() -> TestResult {
    for value in ["", "..", "line\nbreak"] {
        assert_refused("get_message_details", json!({"message_id": value})).await?;
    }
    assert_refused("get_message_details", json!({})).await
}
