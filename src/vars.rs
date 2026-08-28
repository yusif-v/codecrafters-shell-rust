use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

static VARS: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Stores (or overwrites) the shell variable `name` with `value`.
pub fn set(name: &str, value: &str) {
    VARS.lock().unwrap().insert(name.to_string(), value.to_string());
}

/// Returns a clone of the value of `name`, or None if it is not set.
pub fn get(name: &str) -> Option<String> {
    VARS.lock().unwrap().get(name).cloned()
}

/// A valid shell variable name starts with a letter or underscore and
/// contains only ASCII letters, digits, and underscores.
pub fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// Expands `$NAME` occurrences in `word` to the variable's value (empty
/// string when unset). Supports the simple `$VAR` form only; a `$` not
/// followed by an identifier is left literal. Each expanded value stays part
/// of its single argument (no word-splitting of the value).
pub fn expand_word(word: &str) -> String {
    let mut result = String::with_capacity(word.len());
    let mut chars = word.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            let mut name = String::new();
            while let Some(&nc) = chars.peek() {
                if nc == '_' || nc.is_ascii_alphanumeric() {
                    name.push(nc);
                    chars.next();
                } else {
                    break;
                }
            }
            if name.is_empty() {
                result.push('$');
            } else {
                result.push_str(&get(&name).unwrap_or_default());
            }
        } else {
            result.push(c);
        }
    }
    result
}
