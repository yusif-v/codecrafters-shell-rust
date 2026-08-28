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
