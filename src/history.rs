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

/// Appends every non-empty line of `path` to the history, in file order
/// (like bash's `history -r`). Empty lines are skipped. Returns the number
/// of entries added, or an IO error if the file can't be read.
pub fn load_file(path: &str) -> std::io::Result<usize> {
    let content = std::fs::read_to_string(path)?;
    let mut history = HISTORY.lock().unwrap();
    let mut added = 0;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        history.push(line.to_string());
        added += 1;
    }
    Ok(added)
}

/// Writes every history entry to `path`, one per line with a trailing
/// newline (like bash's `history -w`). Creates the file if it doesn't exist.
pub fn save_file(path: &str) -> std::io::Result<()> {
    let history = HISTORY.lock().unwrap();
    let mut content = String::new();
    for cmd in history.iter() {
        content.push_str(cmd);
        content.push('\n');
    }
    std::fs::write(path, content)
}
