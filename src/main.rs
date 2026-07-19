use std::io::{self, BufRead, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

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

        // Tokenize the input respecting quotes/escapes; then strip redirection
        // operators (>, 1>) which are shell syntax, not program arguments.
        let args = tokenize(input);
        if args.is_empty() {
            continue;
        }
        let (cmd_args, redirect) = parse_redirections(&args);
        if cmd_args.is_empty() {
            continue;
        }
        let command = cmd_args[0].as_str();
        let rest = &cmd_args[1..];

        // Builtins are handled directly by the shell. Builtins produce output as
        // a String so it can be redirected to a file like external programs.
        match command {
            "exit" => break,
            "echo" => {
                emit(&rest.join(" "), &redirect);
            }
            "pwd" => {
                let out = match std::env::current_dir() {
                    Ok(dir) => dir.display().to_string(),
                    Err(_) => "pwd: error retrieving current directory".to_string(),
                };
                emit(&out, &redirect);
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
                let target = rest.first().map(|s| s.as_str()).unwrap_or("");
                let out = if is_builtin(target) {
                    format!("{} is a shell builtin", target)
                } else if let Some(full_path) = find_executable(target) {
                    format!("{} is {}", target, full_path)
                } else {
                    format!("{}: not found", target)
                };
                emit(&out, &redirect);
            }
            // Non-builtin commands: try to run an external program.
            _ => {
                if let Some(program) = find_executable(command) {
                    let mut cmd = Command::new(&program);
                    cmd.arg0(command) // argv[0] = command as typed, not the resolved path
                        .args(rest);
                    if let Some(path) = &redirect.stdout {
                        if let Ok(file) = std::fs::File::create(path) {
                            cmd.stdout(Stdio::from(file));
                        }
                    }
                    if let Some(path) = &redirect.stderr {
                        if let Ok(file) = std::fs::File::create(path) {
                            cmd.stderr(Stdio::from(file));
                        }
                    }
                    let status = cmd.status();
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

/// Holds the stdout/stderr redirect targets parsed from a command line.
struct Redirection {
    stdout: Option<String>,
    stderr: Option<String>,
}

/// Extracts redirection operators (`>`, `1>`, `2>`) from a token list.
///
/// Returns the tokens with each redirect operator and its filename removed,
/// plus any stdout/stderr target file paths. Only the FIRST occurrence of each
/// operator is honored. `<` and `>>` are handled by later stages.
fn parse_redirections(args: &[String]) -> (Vec<String>, Redirection) {
    let mut out: Vec<String> = Vec::new();
    let mut redir = Redirection { stdout: None, stderr: None };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let target = if (a == ">" || a == "1>") && i + 1 < args.len() {
            &mut redir.stdout
        } else if a == "2>" && i + 1 < args.len() {
            &mut redir.stderr
        } else {
            out.push(a.clone());
            i += 1;
            continue;
        };
        *target = Some(args[i + 1].clone());
        i += 2; // skip operator and its filename
    }
    (out, redir)
}

/// Writes `text` followed by a newline. If a stdout redirect is set, the output
/// goes to that file (truncating/creating it); otherwise it goes to the
/// terminal. (Builtins emit only to stdout; stderr redirects don't apply to
/// them, which matches shell behavior.)
fn emit(text: &str, redirect: &Redirection) {
    match &redirect.stdout {
        Some(path) => {
            if let Ok(mut f) = std::fs::File::create(path) {
                let _ = writeln!(f, "{}", text);
            }
        }
        None => println!("{}", text),
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

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
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
                } else if ch == '\\' && i + 1 < chars.len() {
                    // Inside double quotes, backslash only escapes \" and \\;
                    // for all other characters the backslash is literal.
                    let next = chars[i + 1];
                    if next == '"' || next == '\\' {
                        current.push(next);
                        i += 1; // consume the escaped character
                    } else {
                        current.push(ch); // literal backslash
                    }
                } else {
                    current.push(ch);
                }
            }
            QuoteState::None => match ch {
                '\'' => quote = QuoteState::Single,
                '"' => quote = QuoteState::Double,
                '\\' => {
                    // Backslash escapes the next character (outside quotes).
                    // The backslash is discarded; the escaped char is literal.
                    if i + 1 < chars.len() {
                        i += 1;
                        current.push(chars[i]);
                    }
                }
                c if c.is_whitespace() => {
                    if !current.is_empty() {
                        args.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(ch),
            },
        }
        i += 1;
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
