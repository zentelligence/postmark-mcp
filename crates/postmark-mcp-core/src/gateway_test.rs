use std::collections::HashSet;

use super::*;

fn every_error() -> Vec<GatewayError> {
    vec![
        GatewayError::Unauthorised,
        GatewayError::RateLimited,
        GatewayError::Rejected {
            status: 422,
            error_code: Some(300),
            message: "Invalid email request".to_owned(),
        },
        GatewayError::Unavailable { status: 503 },
        GatewayError::Timeout,
        GatewayError::Unreachable {
            detail: "connection refused".to_owned(),
        },
        GatewayError::RedirectRefused { status: 302 },
        GatewayError::ResponseTooLarge { limit_bytes: 1024 },
        GatewayError::InvalidResponse {
            detail: "expected a map".to_owned(),
        },
        GatewayError::InvalidRequest {
            detail: "invalid path".to_owned(),
        },
    ]
}

#[test]
fn every_failure_has_a_distinct_code_and_a_remediation() {
    let errors = every_error();
    let codes: HashSet<_> = errors.iter().map(GatewayError::code).collect();

    assert_eq!(codes.len(), errors.len());
    assert!(errors.iter().all(|error| !error.remediation().is_empty()));
}

#[test]
fn rejected_message_preserves_postmarks_explanation() {
    let error = GatewayError::Rejected {
        status: 422,
        error_code: Some(300),
        message: "Invalid email request".to_owned(),
    };

    assert_eq!(
        error.to_string(),
        "Postmark rejected the request (HTTP 422, ErrorCode Some(300)): Invalid email request"
    );
}

#[test]
fn only_a_refusal_before_acceptance_counts_as_certainly_not_sent() {
    for error in every_error() {
        let certain = matches!(
            error,
            GatewayError::Unauthorised
                | GatewayError::RateLimited
                | GatewayError::Rejected { .. }
                | GatewayError::InvalidRequest { .. }
        );

        assert_eq!(error.outcome_is_known(), certain, "{error:?}");
    }
    // A timeout is the case that matters most: Postmark may have accepted it.
    assert!(!GatewayError::Timeout.outcome_is_known());
}

#[test]
fn remediations_name_the_flag_as_well_as_the_variable() {
    let unauthorised = GatewayError::Unauthorised.remediation();
    let redirected = GatewayError::RedirectRefused { status: 302 }.remediation();

    assert!(unauthorised.contains("--server-token") && unauthorised.contains("POSTMARK_SERVER_TOKEN"));
    assert!(redirected.contains("--api-base-url") && redirected.contains("POSTMARK_API_BASE_URL"));
}
