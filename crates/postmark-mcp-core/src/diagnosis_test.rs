use super::*;

const EVERY_VERDICT: [Verdict; 13] = [
    Verdict::Inconclusive,
    Verdict::SuppressedSpamComplaint,
    Verdict::SuppressedHardBounce,
    Verdict::SuppressedManual,
    Verdict::SuppressedOther,
    Verdict::Sandboxed,
    Verdict::Bounced,
    Verdict::Delivered,
    Verdict::Delayed,
    Verdict::Queued,
    Verdict::SentNoDeliveryEvent,
    Verdict::BounceHistoryOnly,
    Verdict::NoRecentMessage,
];

#[test]
fn every_recommendation_names_only_tools_this_server_has() {
    for verdict in EVERY_VERDICT {
        for bounce_id in [None, Some(7)] {
            let text = recommendation(verdict, None, bounce_id);

            assert!(text.len() > 10, "{verdict}");
            for word in text.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                let is_tool_name = word.contains('_') && word != "recent_bounces";
                assert!(!is_tool_name || word == "get_bounce_dump", "{verdict} names {word}");
            }
        }
    }
}
