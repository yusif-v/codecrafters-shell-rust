use std::sync::Mutex;

static HISTORY: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Records a command line as it was entered (before trimming the prompt's
/// trailing spaces; the REPL already trims the line before passing it here).
pub fn record(line: &str) {
    let mut history = HISTORY.lock().unwrap();
    history.push(line.to_string());
}

/// Returns the recorded command lines, most recent last.
pub fn list() -> Vec<String> {
    HISTORY.lock().unwrap().clone()
}
