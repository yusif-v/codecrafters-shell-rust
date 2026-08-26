mod builtins;
mod completion;
mod completions;
mod history;
mod jobs;
mod path;
mod redirection;
mod tokenize;

use rustyline::config::CompletionType;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::Editor;

use crate::completion::ShellHelper;

fn main() -> Result<(), ReadlineError> {
    // "List" completion re-invokes our Completer on each TAB press (so a
    // second TAB completes the next path segment). "Circular" (the default)
    // would instead cycle through the previous TAB's candidates, which is
    // useless for multi-level path completion.
    let config = rustyline::config::Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let mut rl = Editor::<ShellHelper, DefaultHistory>::with_config(config)?;
    let helper = ShellHelper::new();
    rl.set_helper(Some(helper));

    loop {
        // Reap finished background jobs and print their Done lines before
        // drawing the next prompt, so completed jobs appear automatically.
        builtins::reap_background_jobs();

        // The prompt is drawn by rustyline itself. TAB triggers our Completer.
        match rl.readline("$ ") {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                history::record(line);
                let _ = rl.add_history_entry(line);
                builtins::run_command(line);
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
