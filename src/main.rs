use std::io::{self, BufRead, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::os::unix::process::CommandExt;
use std::process::Command;

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        // Display the prompt (from the previous stage)
        print!("$ ");
        stdout.flush().unwrap();

        // Read the user's input
        let mut input = String::new();
        if stdin.lock().read_line(&mut input).unwrap() == 0 {
            break; // EOF
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        // Treat the first whitespace-separated token as the command.
        let mut parts = input.split_whitespace();
        let command = parts.next().unwrap();

        // Builtins are handled directly by the shell.
        match command {
            "exit" => break,
            "echo" => {
                // Print the remaining arguments joined by single spaces.
                let args: Vec<&str> = parts.collect();
                println!("{}", args.join(" "));
            }
            "type" => {
                // Report how the given command would be interpreted.
                let target = parts.next().unwrap_or("");
                if is_builtin(target) {
                    println!("{} is a shell builtin", target);
                } else if let Some(full_path) = find_executable(target) {
                    println!("{} is {}", target, full_path);
                } else {
                    println!("{}: not found", target);
                }
            }
            // Non-builtin commands: try to run an external program.
            _ => {
                let args: Vec<&str> = parts.collect();
                if let Some(program) = find_executable(command) {
                    let status = Command::new(&program)
                        .arg0(command) // argv[0] = command as typed, not the resolved path
                        .args(&args)
                        .status();
                    // If spawning failed, treat it as a not-found command.
                    if status.is_err() {
                        println!("{}: command not found", command);
                    }
                } else {
                    println!("{}: command not found", command);
                }
            }
        }
    }
}

/// Returns true if the given command name is a shell builtin.
fn is_builtin(command: &str) -> bool {
    matches!(command, "echo" | "exit" | "type")
}

/// Searches the directories listed in PATH for an executable file matching
/// `command`. Returns the full path of the first match (a file that exists and
/// has any execute bit set), or None if no executable is found.
fn find_executable(command: &str) -> Option<String> {
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

/// True if the path exists and has at least one execute permission bit set.
fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
