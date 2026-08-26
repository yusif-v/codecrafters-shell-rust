use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::io::FromRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use crate::completions::completions as completion_registry;
use crate::history;
use crate::jobs;
use crate::path::{find_executable, home_dir};
use crate::redirection::{emit, parse_redirections, Redirection};
use crate::tokenize::tokenize;

/// All builtin command names. Used by `type`, the dispatcher, and completion.
pub const BUILTINS: &[&str] = &[
    "echo", "exit", "type", "pwd", "cd", "complete", "jobs", "history",
];

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

/// Creates a new OS pipe, returning `(read_fd, write_fd)`.
fn create_pipe() -> (i32, i32) {
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return (-1, -1);
    }
    (fds[0], fds[1])
}

/// Runs a builtin with its stdout redirected into `write_fd` (a pipe). fd 1 is
/// temporarily repointed at the pipe so the builtin's output (via `println!`)
/// flows into the pipeline; it is restored before returning. If the builtin
/// carries its own stdout redirect, `emit` writes to that file instead, which
/// replaces the pipe — matching external-command behavior.
fn run_builtin_into_pipe(command: &str, rest: &[String], redirect: &Redirection, write_fd: i32) {
    let saved = unsafe { libc::dup(1) };
    unsafe { libc::dup2(write_fd, 1) };
    run_builtin(command, rest, redirect);
    // Flush before restoring fd 1 so buffered output reaches the pipe.
    let _ = std::io::stdout().flush();
    unsafe { libc::dup2(saved, 1) };
    unsafe { libc::close(saved) };
}

/// Dispatches a single builtin command. Builtins run in the shell process;
/// their output is produced as a String and emitted via `emit`, which either
/// writes to a redirect file or prints to stdout.
fn run_builtin(command: &str, rest: &[String], redirect: &Redirection) {
    // Open redirect target files at command time — even if the builtin writes
    // nothing to that stream. A real shell creates the file for `2>`
    // regardless. Append-mode operators must open the file in append mode so
    // empty appends still create the file without truncating existing content.
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
    match command {
        "exit" => std::process::exit(0),
        "echo" => {
            emit(&rest.join(" "), redirect);
        }
        "pwd" => {
            let out = match std::env::current_dir() {
                Ok(dir) => dir.display().to_string(),
                Err(_) => "pwd: error retrieving current directory".to_string(),
            };
            emit(&out, redirect);
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
            emit(&out, redirect);
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
                                    &format!("complete -C '{}' {}", script, name),
                                    redirect,
                                ),
                                None => emit(
                                    &format!(
                                        "complete: {}: no completion specification",
                                        name
                                    ),
                                    redirect,
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
        // List previously executed commands with their line numbers, matching
        // bash's format: the number is right-aligned in a 5-wide field,
        // followed by two spaces and the command.
        "history" => {
            let list = history::list();
            match rest.first().map(|s| s.as_str()) {
                // `history -r <file>...` appends each file's non-empty lines
                // to the in-memory history and produces no output.
                Some("-r") => {
                    for path in &rest[1..] {
                        let _ = history::load_file(path);
                    }
                }
                // `history -w <file>` writes the whole history (one command
                // per line, trailing newline) to the file — creating it if
                // needed — and produces no output.
                Some("-w") => {
                    if let Some(path) = rest.get(1) {
                        let _ = history::save_file(path);
                    }
                }
                // An optional numeric argument limits the listing to the most
                // recent `n` entries, keeping their original line numbers.
                Some(arg) => {
                    let limit = arg.parse::<usize>().unwrap_or(list.len());
                    let start = list.len().saturating_sub(limit);
                    for (index, cmd) in list.iter().enumerate().skip(start) {
                        emit(&format!("{:>5}  {}", index + 1, cmd), redirect);
                    }
                }
                None => {
                    for (index, cmd) in list.iter().enumerate() {
                        emit(&format!("{:>5}  {}", index + 1, cmd), redirect);
                    }
                }
            }
        }
        // Builtins are the only commands dispatched here.
        _ => {}
    }
}

/// Runs a pipeline of two or more commands `cmd1 | cmd2 | ... | cmdN`. Each
/// command's stdout feeds the next command's stdin through an OS pipe (a
/// stdout redirect on a command replaces its pipe). Redirections are applied
/// per command. Builtins run in-process; external commands are spawned as
/// children. After launching everything, children are waited on right-to-left
/// so a downstream command that exits early (e.g. `head -n 5`) lets its
/// upstream writer die of SIGPIPE instead of hanging the pipeline.
fn run_pipeline(segments: &[&[String]]) {
    // Parse redirections for every segment up front; an empty segment means
    // malformed input, so bail out.
    let mut parsed = Vec::with_capacity(segments.len());
    for segment in segments {
        let (cmd_args, redir) = parse_redirections(segment);
        if cmd_args.is_empty() {
            return;
        }
        parsed.push((cmd_args, redir));
    }
    let n = parsed.len();
    if n < 2 {
        return;
    }

    // One OS pipe between each adjacent pair of commands.
    let mut pipes: Vec<(i32, i32)> = Vec::with_capacity(n - 1);
    for _ in 0..n - 1 {
        pipes.push(create_pipe());
    }

    let mut children: Vec<std::process::Child> = Vec::new();

    for i in 0..n {
        let (cmd_args, redir) = &parsed[i];
        let command = &cmd_args[0];
        let rest = &cmd_args[1..];

        // The pipe from the previous command (read end, i>0) and the pipe to
        // the next command (write end, i<n-1). The first command inherits the
        // terminal for stdin; the last inherits it for stdout.
        let in_fd = if i > 0 { Some(pipes[i - 1].0) } else { None };
        let out_fd = if i < n - 1 { Some(pipes[i].1) } else { None };

        if is_builtin(command) {
            // Builtins run in-process and never read stdin, so close the input
            // pipe: a still-writing upstream command gets SIGPIPE instead of
            // blocking forever waiting for a reader that never comes.
            if let Some(fd) = in_fd {
                unsafe { libc::close(fd) };
            }
            match out_fd {
                Some(fd) if redir.stdout.is_none() => {
                    // Point the builtin's output at the pipe, then run it.
                    run_builtin_into_pipe(command, rest, redir, fd);
                    unsafe { libc::close(fd) };
                }
                Some(fd) => {
                    // A stdout redirect replaces the pipe: the builtin writes
                    // to the file and the pipe carries nothing.
                    run_builtin(command, rest, redir);
                    unsafe { libc::close(fd) };
                }
                None => {
                    // Last command: run as normal.
                    run_builtin(command, rest, redir);
                }
            }
            continue;
        }

        let Some(program) = find_executable(command) else {
            println!("{}: command not found", command);
            for &(r, w) in &pipes {
                unsafe {
                    libc::close(r);
                    libc::close(w);
                }
            }
            for mut child in children {
                let _ = child.wait();
            }
            return;
        };
        let mut child = Command::new(&program);
        child.arg0(command).args(rest);
        if let Some(fd) = in_fd {
            child.stdin(Stdio::from(unsafe { std::fs::File::from_raw_fd(fd) }));
        }
        match out_fd {
            Some(fd) if redir.stdout.is_none() => {
                child.stdout(Stdio::from(unsafe { std::fs::File::from_raw_fd(fd) }));
            }
            Some(fd) => {
                if let Some(file) = open_redirect(redir.stdout.as_deref().unwrap(), redir.stdout_append)
                {
                    child.stdout(Stdio::from(file));
                }
                unsafe { libc::close(fd) };
            }
            None => {
                if let Some(path) = &redir.stdout {
                    if let Some(file) = open_redirect(path, redir.stdout_append) {
                        child.stdout(Stdio::from(file));
                    }
                }
            }
        }
        if let Some(path) = &redir.stderr {
            if let Some(file) = open_redirect(path, redir.stderr_append) {
                child.stderr(Stdio::from(file));
            }
        }
        match child.spawn() {
            Ok(spawned) => {
                children.push(spawned);
                // Close the parent's copies of the pipe fds now that the child
                // has its own, so a long-running upstream command still gets
                // SIGPIPE when the downstream one exits.
                drop(child);
            }
            Err(_) => {
                println!("{}: command not found", command);
                for &(r, w) in &pipes {
                    unsafe {
                        libc::close(r);
                        libc::close(w);
                    }
                }
                for mut spawned in children {
                    let _ = spawned.wait();
                }
                return;
            }
        }
    }

    // Wait right-to-left: e.g. `tail -f | head -n 5` lets head finish and the
    // left command die of SIGPIPE on its next write.
    for mut child in children.into_iter().rev() {
        let _ = child.wait();
    }
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

    // A `|` token splits the line into a pipeline of two or more commands.
    // Split on the tokenized `|`, so a `|` inside quotes stays part of an
    // argument.
    if args.contains(&"|".to_string()) {
        let mut segments: Vec<&[String]> = Vec::new();
        let mut start = 0;
        for (i, token) in args.iter().enumerate() {
            if token == "|" {
                segments.push(&args[start..i]);
                start = i + 1;
            }
        }
        segments.push(&args[start..]);
        run_pipeline(&segments);
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

    // Builtins are handled directly by the shell process.
    if is_builtin(command) {
        run_builtin(command, rest, &redirect);
        return;
    }

    // Non-builtin commands: try to run an external program.
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
