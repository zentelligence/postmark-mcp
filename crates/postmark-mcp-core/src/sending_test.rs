use super::*;

fn item(error_code: Option<i64>, message_id: Option<&str>) -> BatchItem {
    BatchItem {
        index: 0,
        to: None,
        error_code,
        message: None,
        message_id: message_id.map(str::to_owned),
        submitted_at: None,
    }
}

#[test]
fn an_answer_without_an_error_code_is_unaccounted_not_refused() {
    let outcome = BatchOutcome::from((
        vec![item(Some(0), Some("a")), item(None, Some("b")), item(Some(406), None)],
        vec!["a@example.com", "b@example.com", "c@example.com"],
    ));

    assert_eq!((outcome.succeeded, outcome.failed, outcome.unaccounted), (1, 1, 1));
}

#[test]
fn answers_beyond_the_messages_sent_are_ignored() {
    let outcome = BatchOutcome::from((
        vec![item(Some(0), Some("a")), item(Some(406), None)],
        vec!["a@example.com"],
    ));

    assert_eq!(
        (outcome.total, outcome.succeeded, outcome.failed, outcome.unaccounted),
        (1, 1, 0, 0)
    );
    assert_eq!(outcome.results.len(), 1);
}

#[test]
fn a_missing_answer_is_unaccounted() {
    let outcome = BatchOutcome::from((vec![item(Some(0), Some("a"))], vec!["a@example.com", "b@example.com"]));

    assert_eq!((outcome.succeeded, outcome.failed, outcome.unaccounted), (1, 0, 1));
}
