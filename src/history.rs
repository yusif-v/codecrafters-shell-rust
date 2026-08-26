use std::sync::Mutex;

static HISTORY: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// How many entries have already been written to a history file by
/// `history -w` / `history -a`; `history -a` only appends entries past this.
static FLUSHED: Mutex<usize> = Mutex::new(0);

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
/// Marks everything written, so a following `-a` appends nothing.
pub fn save_file(path: &str) -> std::io::Result<()> {
    let history = HISTORY.lock().unwrap();
    let content = render(&history[..]);
    std::fs::write(path, content)?;
    *FLUSHED.lock().unwrap() = history.len();
    Ok(())
}

/// Appends entries executed since the last `-w`/`-a` to `path` (like bash's
/// `history -a`). Creates the file if it doesn't exist. Produces no output.
pub fn append_to_file(path: &str) -> std::io::Result<()> {
    use std::io::Write;

    let history = HISTORY.lock().unwrap();
    let mut flushed = FLUSHED.lock().unwrap();
    let start = (*flushed).min(history.len());
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(render(&history[start..]).as_bytes())?;
    *flushed = history.len();
    Ok(())
}

/// Formats history entries as one command per line with a trailing newline.
fn render(entries: &[String]) -> String {
    let mut content = String::new();
    for cmd in entries {
        content.push_str(cmd);
        content.push('\n');
    }
    content
}
