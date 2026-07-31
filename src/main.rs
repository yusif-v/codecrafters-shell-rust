use std::cell::RefCell;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use rustyline::completion::{Completer, Pair};
use rustyline::config::CompletionType;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};

/// All builtin command names. Used by `type`, the dispatcher, and (later)
/// completion.
const BUILTINS: &[&str] = &["echo", "exit", "type", "pwd", "cd"];

fn main() -> Result<(), ReadlineError> {
    // "List" completion re-invokes our Completer on each TAB press (so a
    // second TAB completes the next path segment). "Circular" (the default)
    // would instead cycle through the previous TAB's candidates, which is
    // useless for multi-level path completion.
    let config = rustyline::config::Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let mut rl = Editor::<ShellHelper, DefaultHistory>::with_config(config)?;
    let helper = ShellHelper {
        builtins: BUILTINS.to_vec(),
        last_arg_complete: RefCell::new(None),
    };
    rl.set_helper(Some(helper));

    loop {
        // The prompt is drawn by rustyline itself. TAB triggers our Completer.
        match rl.readline("$ ") {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                run_command(line);
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("error: {:?}", err);
                break;
            }
        }
    }
    Ok(())
}

/// Executes one already-trimmed command line: tokenize, strip redirections,
/// dispatch builtins or spawn an external program.
fn run_command(input: &str) {
    let args = tokenize(input);
    if args.is_empty() {
        return;
    }
    let (cmd_args, redirect) = parse_redirections(&args);
    if cmd_args.is_empty() {
        return;
    }
    let command = cmd_args[0].as_str();
    let rest = &cmd_args[1..];

    // For builtins (run in-process), the shell must still open redirect
    // target files at command time — even if the builtin writes nothing to
    // that stream. A real shell creates the file for `2>` regardless. This
    // mirrors how external commands get their stdio opened below. Append-mode
    // operators must open the file in append mode so empty appends still
    // create the file without truncating existing content.
    if is_builtin(command) {
        if let Some(p) = &redirect.stdout {
            let mut opt = OpenOptions::new();
            if redirect.stdout_append {
                opt.append(true).create(true);
            } else {
                opt.write(true).create(true).truncate(true);
            }
            let _ = opt.open(p);
        }
        if let Some(p) = &redirect.stderr {
            let mut opt = OpenOptions::new();
            if redirect.stderr_append {
                opt.append(true).create(true);
            } else {
                opt.write(true).create(true).truncate(true);
            }
            let _ = opt.open(p);
        }
    }

    // Builtins are handled directly by the shell. Builtins produce output as
    // a String so it can be redirected to a file like external programs.
    match command {
        "exit" => std::process::exit(0),
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
                return;
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
                    let mut opt = OpenOptions::new();
                    if redirect.stdout_append {
                        opt.append(true).create(true);
                    } else {
                        opt.write(true).create(true).truncate(true);
                    }
                    if let Ok(file) = opt.open(path) {
                        cmd.stdout(Stdio::from(file));
                    }
                }
                if let Some(path) = &redirect.stderr {
                    let mut opt = OpenOptions::new();
                    if redirect.stderr_append {
                        opt.append(true).create(true);
                    } else {
                        opt.write(true).create(true).truncate(true);
                    }
                    if let Ok(file) = opt.open(path) {
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

/// rustyline helper providing TAB completion for builtin and external commands.
struct ShellHelper {
    builtins: Vec<&'static str>,
    /// (cursor pos, full line) of the last argument-context completion whose
    /// multiple matches we bell-ed for. Used to tell the first TAB (bell only)
    /// apart from a subsequent TAB on an unchanged line (bell + listing).
    last_arg_complete: RefCell<Option<(usize, String)>>,
}

impl Helper for ShellHelper {}

impl Hinter for ShellHelper {
    type Hint = String;
}

impl Highlighter for ShellHelper {}

impl Validator for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        // Where are we completing? If the cursor is past a space we're
        // typing an argument -> complete filenames in the current directory.
        // Otherwise we're typing the command name -> builtins + PATH.
        let before = &line[..pos.min(line.len())];
        let word_start = before
            .rfind(char::is_whitespace)
            .map(|i| i + 1)
            .unwrap_or(0);
        let partial = &before[word_start..];

        if word_start != 0 {
            // Argument context: complete files relative to current directory.
            // Handles both simple names ("foo") and paths ("dir/sub/").
            let (replace_start, dir, filename_part, is_trailing_slash) = if partial.ends_with('/') {
                // Trailing slash: complete directory contents
                // replace_start = 0 (we'll replace the entire partial)
                // dir = directory name WITHOUT trailing slash (for reading)
                // filename_part = "" (match all files in directory)
                // is_trailing_slash = true (we need to include dir in replacement)
                (0, partial[..partial.len()-1].to_string(), String::new(), true)
            } else if let Some(last_slash) = partial.rfind('/') {
                // Path with prefix: split directory and filename prefix
                // replace_start = position in line where filename starts (after last /)
                // dir = the directory path to read
                // filename_part = the filename prefix we're completing
                // is_trailing_slash = false (normal path completion)
                (last_slash + 1, partial[..last_slash + 1].to_string(), partial[last_slash + 1..].to_string(), false)
            } else {
                // Simple filename in CWD
                (0, ".".to_string(), partial.to_string(), false)
            };

            let mut files: Vec<(String, bool)> = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        // Skip "." and ".." entries
                        if name == "." || name == ".." {
                            continue;
                        }
                        if name.starts_with(&filename_part) {
                            files.push((name.to_string(), entry.path().is_dir()));
                        }
                    }
                }
            }

            if files.is_empty() {
                // No matches: ring bell and leave unchanged.
                let _ = std::io::stdout().write_all(b"\x07");
                let _ = std::io::stdout().flush();
                let display = &partial[replace_start..].to_string();
                return Ok((word_start + replace_start, vec![Pair {
                    display: display.clone(),
                    replacement: display.clone(),
                }]));
            }

            // Sort matches so both single-match completion and the multi-match
            // listing are in alphabetical order.
            let mut sorted_files = files.clone();
            sorted_files.sort_by(|a, b| a.0.cmp(&b.0));

            if sorted_files.len() > 1 {
                // Multiple matches.
                // First: if the matches share a strict common prefix, extend
                // the input to that prefix (the next TAB will disambiguate).
                let names: Vec<String> =
                    sorted_files.iter().map(|(n, _)| n.clone()).collect();
                let lcp = longest_common_prefix(&names);
                if lcp != filename_part {
                    let replacement = if is_trailing_slash {
                        // Include the already-typed directory in the replacement.
                        format!("{}/{}", dir, lcp)
                    } else {
                        lcp.clone()
                    };
                    let candidates = vec![Pair {
                        display: lcp.clone(),
                        replacement,
                    }];
                    return Ok((word_start + replace_start, candidates));
                }
                // Otherwise ring the bell and list every match on its own line,
                // leaving the input unchanged. The first TAB only rings the
                // bell; a subsequent TAB on the same (unchanged) line prints
                // the list.
                let key = (pos, line.to_string());
                let listed =
                    self.last_arg_complete.borrow().as_ref() == Some(&key);
                *self.last_arg_complete.borrow_mut() = Some(key);
                let _ = std::io::stdout().write_all(b"\x07");
                let _ = std::io::stdout().flush();
                if listed {
                    let listing: Vec<String> = sorted_files
                        .iter()
                        .map(|(n, is_dir)| {
                            if *is_dir {
                                format!("{}/", n)
                            } else {
                                n.clone()
                            }
                        })
                        .collect();
                    print!("\r\n{}\r\n", listing.join("  "));
                    let _ = std::io::stdout().flush();
                }
                let display = &partial[replace_start..].to_string();
                return Ok((word_start + replace_start, vec![Pair {
                    display: display.clone(),
                    replacement: display.clone(),
                }]));
            }

            // Single match: complete to the full name. Directories get a
            // trailing slash (no space) so TAB can keep descending; files get
            // a trailing space.
            let (name, is_dir) = &sorted_files[0];
            let suffix = if *is_dir { "/" } else { " " };
            let replacement = if is_trailing_slash {
                // For trailing slash case, we need to include the directory in replacement
                format!("{}/{}{}", dir, name, suffix)
            } else {
                // For normal path completion, we only replace the filename part
                format!("{}{}", name, suffix)
            };
            let candidates = vec![Pair {
                display: name.clone(),
                replacement,
            }];
            return Ok((word_start + replace_start, candidates));
        } else {
            // Command-name context: collect all matching builtins + PATH executables.
            let mut names: Vec<String> = self
                .builtins
                .iter()
                .filter(|b| b.starts_with(partial))
                .map(|b| (*b).to_string())
                .collect();
            for exe in executables_starting_with(partial) {
                names.push(exe);
            }
            names.sort();
            names.dedup();

            // No matches (invalid command): ring the bell and leave the line
            // unchanged.
            if names.is_empty() {
                let _ = std::io::stdout().write_all(b"\x07");
                let _ = std::io::stdout().flush();
                let candidates = vec![Pair {
                    display: partial.to_string(),
                    replacement: partial.to_string(),
                }];
                return Ok((word_start, candidates));
            }

            let single = names.len() == 1;

            if single {
                // One match: complete to the full name with a trailing space.
                let candidates = vec![Pair {
                    display: names[0].clone(),
                    replacement: format!("{} ", names[0]),
                }];
                return Ok((word_start, candidates));
            }

            // Multiple matches: compute the longest common prefix ourselves.
            let lcp = longest_common_prefix(&names);
            if lcp != partial {
                // Strict common prefix (e.g. `xyz_` -> `xyz_foo/...`): insert
                // it in-place. rustyline then re-invokes on the next TAB.
                let candidates = vec![Pair {
                    display: lcp.clone(),
                    replacement: lcp,
                }];
                return Ok((word_start, candidates));
            }

            // No extra prefix (LCP == typed text): ring the bell, print the
            // candidate list on its own line (rustyline's own menu is not
            // captured by the tester, and it does not re-invoke `complete`
            // on a no-op). Return a no-op candidate so the prompt
            // `$ xyz_` stays unchanged.
            let _ = std::io::stdout().write_all(b"\x07");
            let _ = std::io::stdout().flush();
            if !names.is_empty() {
                print!("\r\n{}\r\n", names.join("  "));
                let _ = std::io::stdout().flush();
            }
            let candidates = vec![Pair {
                display: partial.to_string(),
                replacement: partial.to_string(),
            }];
            Ok((word_start, candidates))
        }
    }
}

/// Returns the longest string that is a prefix of every element in `names`.
fn longest_common_prefix(names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    let mut lcp: String = names[0].chars().collect();
    for n in &names[1..] {
        let common: String = lcp
            .chars()
            .zip(n.chars())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a)
            .collect();
        lcp = common;
        if lcp.is_empty() {
            break;
        }
    }
    lcp
}

/// Holds the stdout/stderr redirect targets parsed from a command line, along
/// with whether each is in append mode (`>>`/`1>>`/`2>>`).
struct Redirection {
    stdout: Option<String>,
    stdout_append: bool,
    stderr: Option<String>,
    stderr_append: bool,
}

/// Extracts redirection operators (`>`, `1>`, `2>`, `>>`, `1>>`, `2>>`) from a
/// token list.
///
/// Returns the tokens with each redirect operator and its filename removed,
/// plus the resolved targets. Only the FIRST occurrence of each operator is
/// honored. `<` is handled by a later stage.
fn parse_redirections(args: &[String]) -> (Vec<String>, Redirection) {
    let mut out: Vec<String> = Vec::new();
    let mut redir = Redirection {
        stdout: None,
        stdout_append: false,
        stderr: None,
        stderr_append: false,
    };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if (a == ">" || a == "1>") && i + 1 < args.len() {
            redir.stdout = Some(args[i + 1].clone());
            redir.stdout_append = false;
            i += 2;
        } else if (a == ">>" || a == "1>>") && i + 1 < args.len() {
            redir.stdout = Some(args[i + 1].clone());
            redir.stdout_append = true;
            i += 2;
        } else if a == "2>" && i + 1 < args.len() {
            redir.stderr = Some(args[i + 1].clone());
            redir.stderr_append = false;
            i += 2;
        } else if a == "2>>" && i + 1 < args.len() {
            redir.stderr = Some(args[i + 1].clone());
            redir.stderr_append = true;
            i += 2;
        } else {
            out.push(a.clone());
            i += 1;
        }
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
            let mut opt = OpenOptions::new();
            if redirect.stdout_append {
                opt.append(true).create(true);
            } else {
                opt.write(true).create(true).truncate(true);
            }
            if let Ok(mut f) = opt.open(path) {
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

fn is_builtin(command: &str) -> bool {
    BUILTINS.contains(&command)
}

fn executables_starting_with(partial: &str) -> Vec<String> {
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

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}