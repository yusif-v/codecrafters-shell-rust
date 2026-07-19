use std::io::{self, BufRead, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
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

        // Tokenize the input respecting single quotes. Quoted segments keep
        // their internal whitespace and are concatenated with adjacent
        // segments (quoted or not) into a single argument.
        let args = tokenize(input);
        if args.is_empty() {
            continue;
        }
        let command = args[0].as_str();
        let rest = &args[1..];

        // Builtins are handled directly by the shell.
        match command {
            "exit" => break,
            "echo" => {
                // Arguments are already correctly delimited; join with a space.
                println!("{}", rest.join(" "));
            }
            "pwd" => {
                // Print the absolute path of the current working directory.
                match std::env::current_dir() {
                    Ok(dir) => println!("{}", dir.display()),
                    Err(_) => println!("pwd: error retrieving current directory"),
                }
            }
            "cd" => {
                // Change the current working directory.
                let target = rest.first().map(|s| s.as_str()).unwrap_or("");
                if target.is_empty() {
                    // No argument: behave as a no-op (real shells go home; not
                    // required by this stage).
                    continue;
                }
                // Expand a leading ~ (and ~/...) to the user's home directory.
                let resolved = if target == "~" {
                    home_dir().unwrap_or_else(|| target.to_string())
                } else if let Some(rest_dir) = target.strip_prefix("~/") {
                    match home_dir() {
                        Some(home) => format!("{}/{}", home, rest_dir),
                        None => target.to_string(),
                    }
                } else {
                    target.to_string()
                };
                match std::env::set_current_dir(&resolved) {
                    Ok(()) => {}
                    Err(_) => println!("cd: {}: No such file or directory", target),
                }
            }
            "type" => {
                // Report how the given command would be interpreted.
                let target = rest.first().map(|s| s.as_str()).unwrap_or("");
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
                if let Some(program) = find_executable(command) {
                    let status = Command::new(&program)
                        .arg0(command) // argv[0] = command as typed, not the resolved path
                        .args(rest)
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

/// Splits a command line into arguments, honoring single and double quotes.
///
/// Whitespace outside quotes delimits arguments. Inside quotes (single or
/// double) every character is literal for this stage: spaces are preserved and
/// other quote characters lose their special meaning (so a ' inside "..." and a
/// " inside '...' are literal). Adjacent quoted/unquoted segments concatenate
/// into one argument; empty quotes ('') contribute nothing. (Later stages will
/// add $ / \ interpretation inside double quotes.)
fn tokenize(input: &str) -> Vec<String> {
    #[derive(PartialEq)]
    enum QuoteState {
        None,
        Single,
        Double,
    }

    let mut args: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quote = QuoteState::None;

    for ch in input.chars() {
        match quote {
            QuoteState::Single => {
                if ch == '\'' {
                    quote = QuoteState::None;
                } else {
                    current.push(ch);
                }
            }
            QuoteState::Double => {
                if ch == '"' {
                    quote = QuoteState::None;
                } else {
                    current.push(ch);
                }
            }
            QuoteState::None => match ch {
                '\'' => quote = QuoteState::Single,
                '"' => quote = QuoteState::Double,
                c if c.is_whitespace() => {
                    if !current.is_empty() {
                        args.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(ch),
            },
        }
    }

    // Flush any trailing argument.
    if !current.is_empty() {
        args.push(current);
    }
    args
}

/// Returns the user's home directory as a string, read from the HOME
/// environment variable (falling back to the OS user home dir).
fn home_dir() -> Option<String> {
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

/// Returns true if the given command name is a shell builtin.
fn is_builtin(command: &str) -> bool {
    matches!(command, "echo" | "exit" | "type" | "pwd" | "cd")
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
