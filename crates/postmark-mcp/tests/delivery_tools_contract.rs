//! Contract tests for `search_bounces`, `get_bounce_dump`, `list_suppressions`,
//! and `list_webhooks`.

mod support;

use serde_json::json;
use support::{DEFAULT_STREAM, TestResult, assert_refused, postmark_answering, query, received_query, success};

#[tokio::test]
async fn bounce_search_defaults_to_fifty_results_on_the_configured_stream() -> TestResult {
    let postmark = postmark_answering("GET", "/bounces", json!({"TotalCount": 0, "Bounces": []})).await;

    success(&postmark, "search_bounces", json!({})).await?;

    assert_eq!(
        received_query(&postmark).await?,
        query(&[("count", "50"), ("offset", "0"), ("messagestream", DEFAULT_STREAM)])
    );
    Ok(())
}

#[tokio::test]
async fn bounce_search_sends_every_filter_under_postmarks_parameter_name() -> TestResult {
    let postmark = postmark_answering("GET", "/bounces", json!({"TotalCount": 0, "Bounces": []})).await;

    success(
        &postmark,
        "search_bounces",
        json!({
            "type": "HardBounce",
            "inactive": false,
            "email_filter": "example.com",
            "tag": "welcome",
            "message_id": "0ac29aee",
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
            ("type", "HardBounce"),
            ("inactive", "false"),
            ("emailFilter", "example.com"),
            ("tag", "welcome"),
            ("messageID", "0ac29aee"),
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
async fn bounce_search_returns_full_records_without_message_content() -> TestResult {
    let postmark = postmark_answering(
        "GET",
        "/bounces",
        json!({
            "TotalCount": 1,
            "Bounces": [{
                "RecordType": "Bounce",
                "ID": 692_560_173,
                "Type": "HardBounce",
                "TypeCode": 1,
                "Name": "Hard bounce",
                "Tag": "welcome",
                "MessageID": "2c1b63fe",
                "ServerID": 23,
                "MessageStream": "outbound",
                "Description": "The server was unable to deliver your message",
                "Details": "smtp;550 5.1.1 unknown user",
                "Email": "to@example.com",
                "From": "from@example.com",
                "BouncedAt": "2026-01-02T03:04:05-05:00",
                "DumpAvailable": true,
                "Inactive": true,
                "CanActivate": true,
                "Subject": "Hello",
                "Content": "secret original message",
            }],
        }),
    )
    .await;

    let result = success(&postmark, "search_bounces", json!({})).await?;

    assert_eq!(
        result,
        json!({
            "total_count": 1,
            "bounces": [{
                "bounce_id": 692_560_173,
                "type": "HardBounce",
                "type_code": 1,
                "name": "Hard bounce",
                "tag": "welcome",
                "message_id": "2c1b63fe",
                "message_stream": "outbound",
                "description": "The server was unable to deliver your message",
                "details": "smtp;550 5.1.1 unknown user",
                "email": "to@example.com",
                "from": "from@example.com",
                "bounced_at": "2026-01-02T03:04:05-05:00",
                "dump_available": true,
                "inactive": true,
                "can_activate": true,
                "subject": "Hello",
            }],
        })
    );
    Ok(())
}

#[tokio::test]
async fn bounce_search_refuses_invalid_filters() -> TestResult {
    for arguments in [
        json!({"type": "NotABounce"}),
        json!({"count": 501}),
        json!({"count": 500, "offset": 9501}),
        json!({"from_date": "yesterday"}),
        json!({"email_filter": ""}),
    ] {
        assert_refused("search_bounces", arguments).await?;
    }
    Ok(())
}

#[tokio::test]
async fn bounce_dump_returns_the_transcript() -> TestResult {
    let postmark = postmark_answering("GET", "/bounces/99/dump", json!({"Body": "SMTP dump data"})).await;

    let result = success(&postmark, "get_bounce_dump", json!({"bounce_id": 99})).await?;

    assert_eq!(
        result,
        json!({"bounce_id": 99, "available": true, "body": "SMTP dump data"})
    );
    Ok(())
}

#[tokio::test]
async fn bounce_dump_reports_an_expired_transcript_as_unavailable() -> TestResult {
    let postmark = postmark_answering("GET", "/bounces/99/dump", json!({"Body": ""})).await;

    let result = success(&postmark, "get_bounce_dump", json!({"bounce_id": 99})).await?;

    assert_eq!(result, json!({"bounce_id": 99, "available": false}));
    Ok(())
}

#[tokio::test]
async fn bounce_dump_refuses_ids_postmark_cannot_have_issued() -> TestResult {
    assert_refused("get_bounce_dump", json!({"bounce_id": 0})).await?;
    assert_refused("get_bounce_dump", json!({"bounce_id": -1})).await?;
    assert_refused("get_bounce_dump", json!({"bounce_id": "99/../1"})).await
}

#[tokio::test]
async fn suppressions_default_to_the_configured_stream() -> TestResult {
    let route = format!("/message-streams/{DEFAULT_STREAM}/suppressions/dump");
    let postmark = postmark_answering("GET", &route, json!({"Suppressions": []})).await;

    let result = success(&postmark, "list_suppressions", json!({})).await?;

    assert_eq!(result, json!({"message_stream": DEFAULT_STREAM, "suppressions": []}));
    assert_eq!(received_query(&postmark).await?, query(&[]));
    Ok(())
}

#[tokio::test]
async fn suppressions_send_every_filter_under_postmarks_parameter_name() -> TestResult {
    let postmark = postmark_answering(
        "GET",
        "/message-streams/broadcast/suppressions/dump",
        json!({
            "Suppressions": [{
                "EmailAddress": "to@example.com",
                "SuppressionReason": "HardBounce",
                "Origin": "Recipient",
                "CreatedAt": "2026-01-02T03:04:05-05:00",
            }],
        }),
    )
    .await;

    let result = success(
        &postmark,
        "list_suppressions",
        json!({
            "message_stream": "broadcast",
            "suppression_reason": "HardBounce",
            "origin": "Recipient",
            "email_address": "to@example.com",
            "from_date": "2026-01-01",
            "to_date": "2026-01-31",
        }),
    )
    .await?;

    assert_eq!(
        received_query(&postmark).await?,
        query(&[
            ("SuppressionReason", "HardBounce"),
            ("Origin", "Recipient"),
            ("EmailAddress", "to@example.com"),
            ("fromdate", "2026-01-01"),
            ("todate", "2026-01-31"),
        ])
    );
    assert_eq!(
        result,
        json!({
            "message_stream": "broadcast",
            "suppressions": [{
                "email_address": "to@example.com",
                "suppression_reason": "HardBounce",
                "origin": "Recipient",
                "created_at": "2026-01-02T03:04:05-05:00",
            }],
        })
    );
    Ok(())
}

#[tokio::test]
async fn suppressions_refuse_invalid_filters() -> TestResult {
    for arguments in [
        json!({"message_stream": "../servers"}),
        json!({"suppression_reason": "Unknown"}),
        json!({"origin": "Nobody"}),
        json!({"to_date": "2026-1-1"}),
    ] {
        assert_refused("list_suppressions", arguments).await?;
    }
    Ok(())
}

#[tokio::test]
async fn webhooks_never_expose_credentials_or_custom_headers() -> TestResult {
    let postmark = postmark_answering(
        "GET",
        "/webhooks",
        json!({
            "Webhooks": [{
                "ID": 1_234_567,
                "Url": "https://urluser:urlpass@www.example.com/webhook?token=querysecret#fragmentsecret",
                "MessageStream": "outbound",
                "Status": "verified",
                "HttpAuth": {"Username": "authuser", "Password": "authpass"},
                "HttpHeaders": [{"Name": "X-Api-Key", "Value": "headersecret"}],
                "Triggers": {
                    "Open": {"Enabled": true, "PostFirstOpenOnly": false},
                    "Click": {"Enabled": true},
                    "Delivery": {"Enabled": false},
                    "Bounce": {"Enabled": false, "IncludeContent": false},
                    "SpamComplaint": {"Enabled": true, "IncludeContent": false},
                    "SubscriptionChange": {"Enabled": false},
                },
            }],
        }),
    )
    .await;

    let result = success(&postmark, "list_webhooks", json!({})).await?;

    assert_eq!(
        result,
        json!({
            "webhooks": [{
                "id": 1_234_567,
                "url": "https://www.example.com/webhook",
                "message_stream": "outbound",
                "status": "verified",
                "triggers": ["Open", "Click", "SpamComplaint"],
            }],
        })
    );
    let serialised = result.to_string();
    for secret in [
        "urlpass",
        "authpass",
        "authuser",
        "headersecret",
        "X-Api-Key",
        "querysecret",
        "fragmentsecret",
    ] {
        assert!(!serialised.contains(secret), "{secret} reached the tool result");
    }
    Ok(())
}

#[tokio::test]
async fn webhooks_cover_all_streams_unless_one_is_requested() -> TestResult {
    let postmark = postmark_answering("GET", "/webhooks", json!({"Webhooks": []})).await;

    success(&postmark, "list_webhooks", json!({})).await?;

    assert_eq!(received_query(&postmark).await?, query(&[]));
    Ok(())
}

#[tokio::test]
async fn webhooks_can_be_filtered_by_stream() -> TestResult {
    let postmark = postmark_answering("GET", "/webhooks", json!({"Webhooks": []})).await;

    success(&postmark, "list_webhooks", json!({"message_stream": "broadcast"})).await?;

    assert_eq!(
        received_query(&postmark).await?,
        query(&[("MessageStream", "broadcast")])
    );
    Ok(())
}

#[tokio::test]
async fn webhooks_refuse_invalid_streams() -> TestResult {
    assert_refused("list_webhooks", json!({"message_stream": "a/b"})).await
}
