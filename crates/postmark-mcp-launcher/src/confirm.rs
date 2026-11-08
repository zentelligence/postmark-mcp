//! Asking the person at the terminal before a change.

use std::io::Write;

/// Asks `question` and reads one line, treating anything but yes as no, as is
/// an input that is closed or cannot be read.
pub(crate) fn confirmed(question: &str) -> bool {
    print!("{question} [y/N]: ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
}
