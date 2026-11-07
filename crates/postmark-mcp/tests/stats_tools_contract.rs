//! Contract tests for `get_delivery_stats`.

mod support;

use serde_json::json;
use support::{TestResult, assert_refused, postmark_answering, query, received_query, success};

fn overview() -> serde_json::Value {
    json!({
        "Sent": 200,
        "Bounced": 10,
        "SMTPApiErrors": 1,
        "BounceRate": 5.0,
        "SpamComplaints": 2,
        "SpamComplaintsRate": 1.0,
        "Opens": 150,
        "UniqueOpens": 50,
        "TotalClicks": 30,
        "UniqueLinksClicked": 20,
        "TotalTrackedLinksSent": 80,
        "Tracked": 100,
        "WithLinkTracking": 90,
        "WithOpenTracking": 80,
    })
}

#[tokio::test]
async fn summary_is_the_default_and_reports_headline_rates() -> TestResult {
    let postmark = postmark_answering("GET", "/stats/outbound", overview()).await;

    let result = success(&postmark, "get_delivery_stats", json!({})).await?;

    assert_eq!(result.get("stat"), Some(&json!("summary")));
    assert_eq!(
        result.get("rates"),
        Some(&json!({
            "open_rate_percent": 62.5,
            "click_rate_percent": 25.0,
            "bounce_rate_percent": 5.0,
            "spam_rate_percent": 1.0,
        }))
    );
    assert_eq!(result.pointer("/totals/Sent"), Some(&json!(200)));
    Ok(())
}

#[tokio::test]
async fn rates_with_missing_figures_are_omitted_rather_than_reported_as_zero() -> TestResult {
    let postmark = postmark_answering(
        "GET",
        "/stats/outbound",
        json!({"Sent": 5, "Bounced": 1, "WithOpenTracking": 0}),
    )
    .await;

    let result = success(&postmark, "get_delivery_stats", json!({"stat": "summary"})).await?;

    assert_eq!(result.get("rates"), Some(&json!({"bounce_rate_percent": 20.0})));
    Ok(())
}

#[tokio::test]
async fn rates_are_rounded_to_one_decimal_place() -> TestResult {
    let postmark = postmark_answering("GET", "/stats/outbound", json!({"Sent": 3, "Bounced": 1})).await;

    let result = success(&postmark, "get_delivery_stats", json!({})).await?;

    assert_eq!(result.pointer("/rates/bounce_rate_percent"), Some(&json!(33.3)));
    Ok(())
}

#[tokio::test]
async fn each_stat_reads_its_documented_endpoint() -> TestResult {
    for (stat, route) in [
        ("overview", "/stats/outbound"),
        ("sent", "/stats/outbound/sends"),
        ("bounces", "/stats/outbound/bounces"),
        ("spam", "/stats/outbound/spam"),
        ("tracked", "/stats/outbound/tracked"),
        ("opens", "/stats/outbound/opens"),
        ("open_platforms", "/stats/outbound/opens/platforms"),
        ("open_clients", "/stats/outbound/opens/emailclients"),
        ("open_read_times", "/stats/outbound/opens/readtimes"),
        ("clicks", "/stats/outbound/clicks"),
        ("click_browsers", "/stats/outbound/clicks/browserfamilies"),
        ("click_platforms", "/stats/outbound/clicks/platforms"),
        ("click_location", "/stats/outbound/clicks/location"),
    ] {
        let postmark = postmark_answering("GET", route, json!({"Total": 1})).await;

        let result = success(&postmark, "get_delivery_stats", json!({"stat": stat})).await?;

        assert_eq!(result.get("stat"), Some(&json!(stat)), "{route}");
        assert_eq!(result.get("rates"), None, "{route}");
    }
    Ok(())
}

#[tokio::test]
async fn breakdowns_keep_dynamic_names_and_leave_out_the_daily_series() -> TestResult {
    let postmark = postmark_answering(
        "GET",
        "/stats/outbound/opens/emailclients",
        json!({
            "Days": [{"Date": "2026-01-01", "Apple Mail": 3, "Outlook 2010": 1}],
            "Apple Mail": 3,
            "Outlook 2010": 1,
        }),
    )
    .await;

    let result = success(&postmark, "get_delivery_stats", json!({"stat": "open_clients"})).await?;

    assert_eq!(
        result,
        json!({"stat": "open_clients", "totals": {"Apple Mail": 3, "Outlook 2010": 1}})
    );
    Ok(())
}

#[tokio::test]
async fn stats_send_filters_and_cover_all_streams_unless_one_is_requested() -> TestResult {
    let postmark = postmark_answering("GET", "/stats/outbound/sends", json!({"Sent": 1})).await;
    success(&postmark, "get_delivery_stats", json!({"stat": "sent"})).await?;
    assert_eq!(received_query(&postmark).await?, query(&[]));

    let postmark = postmark_answering("GET", "/stats/outbound/sends", json!({"Sent": 1})).await;
    success(
        &postmark,
        "get_delivery_stats",
        json!({
            "stat": "sent",
            "tag": "welcome",
            "from_date": "2026-01-01",
            "to_date": "2026-01-31",
            "message_stream": "broadcast",
        }),
    )
    .await?;
    assert_eq!(
        received_query(&postmark).await?,
        query(&[
            ("tag", "welcome"),
            ("fromdate", "2026-01-01"),
            ("todate", "2026-01-31"),
            ("messagestream", "broadcast"),
        ])
    );
    Ok(())
}

#[tokio::test]
async fn stats_refuse_invalid_filters() -> TestResult {
    for arguments in [
        json!({"stat": "unknown"}),
        json!({"stat": "openPlatforms"}),
        json!({"from_date": "2026-02-01", "to_date": "2026-01-01"}),
        json!({"message_stream": "a b"}),
    ] {
        assert_refused("get_delivery_stats", arguments).await?;
    }
    Ok(())
}
