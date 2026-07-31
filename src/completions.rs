use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Programmable completions registered via `complete`. Maps a command name to
/// its registered completer script path. The shell is single-threaded; the
/// lock is only for safe shared access.
static COMPLETIONS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

pub fn completions() -> &'static Mutex<HashMap<String, String>> {
    COMPLETIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Longest common prefix of a set of candidate words.
pub fn common_prefix(words: &[String]) -> String {
    let Some(first) = words.first() else {
        return String::new();
    };
    let mut end = first.len();
    for w in &words[1..] {
        let max = end.min(w.len());
        let mut i = 0;
        while i < max && first.as_bytes()[i] == w.as_bytes()[i] {
            i += 1;
        }
        end = i;
        if end == 0 {
            break;
        }
    }
    first[..end].to_string()
}
