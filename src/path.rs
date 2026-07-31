use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Returns the user's home directory as a string, read from the HOME
/// environment variable (falling back to the OS user home dir).
pub fn home_dir() -> Option<String> {
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return Some(home);
        }
    }
    std::env::var("USER")
        .ok()
        .map(|u| format!("/Users/{}", u))
        .filter(|p| Path::new(p).is_dir())
}

pub fn executables_starting_with(partial: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with(partial) {
                    continue;
                }
                if is_executable(&entry.path()) && !found.iter().any(|f| f == name.as_ref()) {
                    found.push(name.to_string());
                }
            }
        }
    }
    found.sort();
    found
}

/// Searches the directories listed in PATH for an executable file matching
/// `command`. Returns the full path of the first match (a file that exists and
/// has any execute bit set), or None if no executable is found.
pub fn find_executable(command: &str) -> Option<String> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = Path::new(dir).join(command);
        if is_executable(&candidate) {
            return candidate.to_str().map(|s| s.to_string());
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
