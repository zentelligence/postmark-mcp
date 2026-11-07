//! Outbound delivery statistics.

use std::collections::BTreeMap;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Number;
use serde_with::skip_serializing_none;

use crate::wire::{JsonObject, vocabulary};
use crate::{DateRange, FilterText, GatewayError, MessageStreamId};

vocabulary! {
    /// Which statistic to report.
    DeliveryStat {
        Summary => "summary",
        Overview => "overview",
        Sent => "sent",
        Bounces => "bounces",
        Spam => "spam",
        Tracked => "tracked",
        Opens => "opens",
        OpenPlatforms => "open_platforms",
        OpenClients => "open_clients",
        OpenReadTimes => "open_read_times",
        Clicks => "clicks",
        ClickBrowsers => "click_browsers",
        ClickPlatforms => "click_platforms",
        ClickLocation => "click_location",
    }
}

/// Which statistics to read.
#[derive(Clone, Debug)]
pub struct StatsQuery {
    /// The statistic to report.
    pub stat: DeliveryStat,
    /// Restricts results to one tag.
    pub tag: Option<FilterText>,
    /// Restricts results to one stream. All streams are covered otherwise.
    pub message_stream: Option<MessageStreamId>,
    /// Restricts results to a date range. All time is covered otherwise.
    pub dates: DateRange,
}

/// Headline rates, each a percentage to one decimal place. A rate missing either
/// figure, or with a denominator of zero, is omitted rather than reported as
/// zero, because no data is not the same as a rate of nothing.
#[skip_serializing_none]
#[derive(Clone, Debug, JsonSchema, PartialEq, Serialize)]
pub struct DeliveryRates {
    /// Unique opens as a share of emails with open tracking.
    pub open_rate_percent: Option<f64>,
    /// Unique links clicked as a share of tracked links sent.
    pub click_rate_percent: Option<f64>,
    /// Bounces as a share of sent emails.
    pub bounce_rate_percent: Option<f64>,
    /// Spam complaints as a share of sent emails.
    pub spam_rate_percent: Option<f64>,
}

fn rate(totals: &BTreeMap<String, Number>, numerator: &str, denominator: &str) -> Option<f64> {
    let value = |name: &str| totals.get(name).and_then(Number::as_f64);
    let denominator = value(denominator).filter(|total| *total > 0.0)?;
    let percent = value(numerator)? / denominator * 100.0;
    Some((percent * 10.0).round() / 10.0)
}

impl From<&BTreeMap<String, Number>> for DeliveryRates {
    fn from(totals: &BTreeMap<String, Number>) -> Self {
        Self {
            open_rate_percent: rate(totals, "UniqueOpens", "WithOpenTracking"),
            click_rate_percent: rate(totals, "UniqueLinksClicked", "TotalTrackedLinksSent"),
            bounce_rate_percent: rate(totals, "Bounced", "Sent"),
            spam_rate_percent: rate(totals, "SpamComplaints", "Sent"),
        }
    }
}

/// A statistic and its totals. Totals keep Postmark's names, since many are
/// dynamic, such as one per email client.
#[skip_serializing_none]
#[derive(Clone, Debug, JsonSchema, PartialEq, Serialize)]
pub struct DeliveryStats {
    /// The statistic reported.
    pub stat: DeliveryStat,
    /// Totals for the period, by Postmark's name for each.
    pub totals: BTreeMap<String, Number>,
    /// Headline rates, for the `summary` statistic only.
    pub rates: Option<DeliveryRates>,
}

/// Keeps the numeric totals. This drops the `Days` series, whose size grows with
/// the date range, and which no tool reports.
impl From<(DeliveryStat, JsonObject)> for DeliveryStats {
    fn from((stat, response): (DeliveryStat, JsonObject)) -> Self {
        let totals: BTreeMap<_, _> = response
            .into_iter()
            .filter_map(|(name, value)| value.as_number().cloned().map(|number| (name, number)))
            .collect();
        let rates = (stat == DeliveryStat::Summary).then(|| DeliveryRates::from(&totals));
        Self { stat, totals, rates }
    }
}

/// Reads delivery statistics.
#[async_trait]
pub trait ReadsStats: Send + Sync {
    /// Reads one statistic.
    ///
    /// # Errors
    ///
    /// Returns a [`GatewayError`] when the request fails.
    async fn delivery_stats(&self, query: &StatsQuery) -> Result<DeliveryStats, GatewayError>;
}
