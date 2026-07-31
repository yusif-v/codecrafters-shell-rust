use std::fs::OpenOptions;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use crate::completions::completions as completion_registry;
use crate::jobs;
use crate::path::{find_executable, home_dir};
use crate::redirection::{emit, parse_redirections};
use crate::tokenize::tokenize;

/// All builtin command names. Used by `type`, the dispatcher, and completion.
pub const BUILTINS: &[&str] = &["echo", "exit", "type", "pwd", "cd", "complete", "jobs"];

pub fn is_builtin(command: &str) -> bool {
    BUILTINS.contains(&command)
}

/// Executes one already-trimmed command line: tokenize, strip redirections,
/// dispatch builtins or spawn an external program. A trailing `&` token runs
/// the command in the background (the shell doesn't wait for it to finish).
pub fn run_command(input: &str) {
    let args = tokenize(input);
    if args.is_empty() {
        return;
    }
    let (mut cmd_args, redirect) = parse_redirections(&args);

    // A trailing `&` (as the final token AND at the end of the raw line, so
    // that `echo "&"` isn't mistaken for a background request) runs the
    // remaining command in the background.
    let background = cmd_args.last().map(|s| s.as_str()) == Some("&")
        && input.trim_end().ends_with('&');
    if background {
        cmd_args.pop();
    }

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
        // Programmable completion builtin. `complete -C <script> <name>`
        // registers a completer script for <name>; `complete -p <name>`
        // prints the registered specification in normalized form, or an
        // error if none exists.
        "complete" => {
            let mut args = rest.iter();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    // Register the following script path against every
                    // command name that follows it.
                    "-C" => {
                        if let Some(script) = args.next() {
                            let mut specs = completion_registry().lock().unwrap();
                            for name in args.by_ref() {
                                if name.starts_with('-') {
                                    break;
                                }
                                specs.insert(name.clone(), script.clone());
                            }
                        }
                    }
                    // Print the registered specification for every following
                    // command name.
                    "-p" => {
                        let specs = completion_registry().lock().unwrap();
                        for name in args.by_ref() {
                            if name.starts_with('-') {
                                break;
                            }
                            match specs.get(name) {
                                Some(script) => emit(
                                    &format!(
                                        "complete -C '{}' {}",
                                        script, name
                                    ),
                                    &redirect,
                                ),
                                None => emit(
                                    &format!(
                                        "complete: {}: no completion specification",
                                        name
                                    ),
                                    &redirect,
                                ),
                            }
                        }
                    }
                    // Remove the registered specification for every following
                    // command name. Removing an unknown command is a no-op.
                    "-r" => {
                        let mut specs = completion_registry().lock().unwrap();
                        for name in args.by_ref() {
                            if name.starts_with('-') {
                                break;
                            }
                            specs.remove(name);
                        }
                    }
                    _ => {}
                }
            }
        }
        // List every background job the shell has started. Jobs that are no
        // longer known (e.g. done and reaped) simply don't appear. The most
        // recently started job gets the `+` marker; others get `-`.
        "jobs" => {
            let list = jobs::list_jobs();
            for (index, job) in list.iter().enumerate() {
                let marker = if index + 1 == list.len() { "+" } else { "-" };
                // Status is a fixed-width 24-char field; the command follows
                // it directly ("Running" + 17 trailing spaces).
                println!("[{}]{}  {:<24}{}", job.id, marker, "Running", job.command);
            }
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
                if background {
                    // Run without waiting: spawn the child and report its job
                    // number and PID, then let the shell return to the prompt
                    // immediately.
                    match cmd.spawn() {
                        Ok(child) => {
                            let id = jobs::add_job(child.id(), input.to_string());
                            println!("[{}] {}", id, child.id());
                        }
                        Err(_) => println!("{}: command not found", command),
                    }
                } else {
                    let status = cmd.status();
                    if status.is_err() {
                        println!("{}: command not found", command);
                    }
                }
            } else {
                println!("{}: command not found", command);
            }
        }
    }
}
