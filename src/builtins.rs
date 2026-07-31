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

/// The marker for the job at `index` in a list of `len` jobs: the most
/// recently started job gets `+`, the second most recent gets `-`, and every
/// other job a blank marker.
fn job_marker(index: usize, len: usize) -> &'static str {
    if index + 1 == len {
        "+"
    } else if index + 2 == len {
        "-"
    } else {
        " "
    }
}

/// Prints one job line. Status is a fixed-width 24-char field; the command
/// follows it directly (e.g. "Running" + 17 trailing spaces). Done entries
/// omit the trailing `&` recorded at launch.
fn print_job(job: &jobs::JobSnapshot, index: usize, len: usize) {
    let (status, command) = match job.status {
        jobs::JobStatus::Running => ("Running", job.command.as_str()),
        jobs::JobStatus::Done => {
            let stripped = job.command.trim_end().trim_end_matches('&');
            ("Done", stripped.trim_end())
        }
    };
    println!("[{}]{}  {:<24}{}", job.id, job_marker(index, len), status, command);
}

/// Opens a redirect target file, honoring append mode. Returns None if the
/// file can't be opened.
fn open_redirect(path: &str, append: bool) -> Option<std::fs::File> {
    let mut opt = OpenOptions::new();
    if append {
        opt.append(true).create(true);
    } else {
        opt.write(true).create(true).truncate(true);
    }
    opt.open(path).ok()
}

/// Runs a two-command pipeline `left | right`: the left command's stdout feeds
/// the right command's stdin. Redirections are applied per side; a stdout
/// redirect on the left replaces the pipe, otherwise stdout is piped.
fn run_pipeline(left: &[String], right: &[String]) {
    let (left_args, left_redir) = parse_redirections(left);
    let (right_args, right_redir) = parse_redirections(right);
    if left_args.is_empty() || right_args.is_empty() {
        return;
    }

    // Left command: stdout goes to the pipe unless it redirects stdout.
    let left_cmd = &left_args[0];
    let Some(left_program) = find_executable(left_cmd) else {
        println!("{}: command not found", left_cmd);
        return;
    };
    let mut left = Command::new(&left_program);
    left.arg0(left_cmd).args(&left_args[1..]);
    if let Some(path) = &left_redir.stderr {
        if let Some(file) = open_redirect(path, left_redir.stderr_append) {
            left.stderr(Stdio::from(file));
        }
    }
    if let Some(path) = &left_redir.stdout {
        if let Some(file) = open_redirect(path, left_redir.stdout_append) {
            left.stdout(Stdio::from(file));
        }
    } else {
        left.stdout(Stdio::piped());
    }
    let mut left_child = match left.spawn() {
        Ok(child) => child,
        Err(_) => {
            println!("{}: command not found", left_cmd);
            return;
        }
    };

    // Right command: stdin comes from the left command's pipe.
    let right_cmd = &right_args[0];
    let Some(right_program) = find_executable(right_cmd) else {
        println!("{}: command not found", right_cmd);
        // Drop the pipe reader so a writing left command doesn't block on it.
        let _ = left_child.stdout.take();
        let _ = left_child.wait();
        return;
    };
    let mut right = Command::new(&right_program);
    right.arg0(right_cmd).args(&right_args[1..]);
    if let Some(pipe) = left_child.stdout.take() {
        right.stdin(Stdio::from(pipe));
    }
    if let Some(path) = &right_redir.stdout {
        if let Some(file) = open_redirect(path, right_redir.stdout_append) {
            right.stdout(Stdio::from(file));
        }
    }
    if let Some(path) = &right_redir.stderr {
        if let Some(file) = open_redirect(path, right_redir.stderr_append) {
            right.stderr(Stdio::from(file));
        }
    }
    let mut right_child = match right.spawn() {
        Ok(child) => child,
        Err(_) => {
            println!("{}: command not found", right_cmd);
            let _ = left_child.wait();
            return;
        }
    };
    // Drop the parent's copy of the pipe read end now that the right command
    // has its own. Otherwise, when the right command exits, the read end stays
    // open here and a long-running left command (e.g. `tail -f`) never gets
    // SIGPIPE, so the pipeline hangs.
    drop(right);
    // Wait for the right command first: e.g. `tail -f | head -n 5` lets head
    // finish and the left command die of SIGPIPE on its next write.
    let _ = right_child.wait();
    let _ = left_child.wait();
}

/// Reaps finished background jobs, printing a Done line for each one that
/// completed. Called before every prompt so completed jobs appear right after
/// the previous command's output, without needing to run `jobs`.
pub fn reap_background_jobs() {
    let list = jobs::reap();
    let len = list.len();
    for (index, job) in list.iter().enumerate() {
        if job.status == jobs::JobStatus::Done {
            print_job(job, index, len);
        }
    }
}

/// Executes one already-trimmed command line: tokenize, strip redirections,
/// dispatch builtins or spawn an external program. A trailing `&` token runs
/// the command in the background (the shell doesn't wait for it to finish).
pub fn run_command(input: &str) {
    let args = tokenize(input);
    if args.is_empty() {
        return;
    }

    // A `|` token splits the line into a two-command pipeline. Split on the
    // tokenized `|`, so a `|` inside quotes stays part of an argument.
    if let Some(pipe) = args.iter().position(|t| t == "|") {
        run_pipeline(&args[..pipe], &args[pipe + 1..]);
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
        // List every background job the shell is tracking. Finished jobs are
        // shown with status Done (and no trailing `&`), then dropped from the
        // table so they don't appear in later calls.
        "jobs" => {
            let list = jobs::reap();
            for (index, job) in list.iter().enumerate() {
                print_job(job, index, list.len());
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
                            let pid = child.id();
                            let id = jobs::add_job(child, input.to_string());
                            println!("[{}] {}", id, pid);
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
