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
