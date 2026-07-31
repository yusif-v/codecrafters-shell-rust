use std::cell::RefCell;
use std::io::Write;
use std::process::Command;

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper};

use crate::builtins::BUILTINS;
use crate::completions::common_prefix;
use crate::completions::completions as completion_registry;
use crate::path::executables_starting_with;

/// rustyline helper providing TAB completion for builtin and external commands.
pub struct ShellHelper {
    pub builtins: Vec<&'static str>,
    /// (cursor pos, full line) of the last argument-context completion whose
    /// multiple matches we bell-ed for. Used to tell the first TAB (bell only)
    /// apart from a subsequent TAB on an unchanged line (bell + listing).
    pub last_arg_complete: RefCell<Option<(usize, String)>>,
}

impl ShellHelper {
    pub fn new() -> Self {
        ShellHelper {
            builtins: BUILTINS.to_vec(),
            last_arg_complete: RefCell::new(None),
        }
    }
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
            // Argument context. If a programmable completion is registered for
            // the command, run its script and use the stdout lines as
            // candidates (replacing the current word).
            let words_before: Vec<&str> =
                before[..word_start].split_whitespace().collect();
            let command = words_before.first().copied().unwrap_or("");
            if let Some(script) = completion_registry().lock().unwrap().get(command).cloned() {
                // argv[3] is the word immediately before the one being
                // completed. The command name itself doesn't count, so when
                // the current word is the first argument, pass an empty
                // string.
                let prev_word = words_before.last().copied().unwrap_or("");
                let candidates: Vec<String> = match Command::new(&script)
                    .arg(command)
                    .arg(partial)
                    .arg(prev_word)
                    // COMP_LINE/COMP_POINT are set on the completer process
                    // only (not persisted in the shell's own environment).
                    // COMP_POINT is the zero-based byte index of the cursor
                    // in the line.
                    .env("COMP_LINE", line)
                    .env("COMP_POINT", pos.to_string())
                    .output()
                {
                    Ok(output) => String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect(),
                    Err(_) => Vec::new(),
                };
                if candidates.len() == 1 {
                    // Single candidate: complete to it with a trailing space.
                    let candidate = candidates[0].clone();
                    let replacement = format!("{} ", candidate);
                    return Ok((word_start, vec![Pair {
                        display: candidate,
                        replacement,
                    }]));
                }
                if !candidates.is_empty() {
                    // Longest common prefix: if the candidates share a prefix
                    // longer than what the user typed, complete to it (no
                    // bell, no trailing space).
                    let lcp = common_prefix(&candidates);
                    if lcp.len() > partial.len() {
                        return Ok((word_start, vec![Pair {
                            display: lcp.clone(),
                            replacement: lcp,
                        }]));
                    }
                    // No extension of the current input: the first TAB rings
                    // the bell (no unique match); a subsequent TAB on the same
                    // unchanged line prints them sorted (two-space separated)
                    // and reprints the prompt with the original input.
                    let mut sorted = candidates;
                    sorted.sort();
                    let key = (pos, line.to_string());
                    let listed =
                        self.last_arg_complete.borrow().as_ref() == Some(&key);
                    *self.last_arg_complete.borrow_mut() = Some(key);
                    let _ = std::io::stdout().write_all(b"\x07");
                    let _ = std::io::stdout().flush();
                    if listed {
                        print!("\r\n{}\r\n", sorted.join("  "));
                        let _ = std::io::stdout().flush();
                    }
                    let display = partial.to_string();
                    return Ok((word_start, vec![Pair {
                        display: display.clone(),
                        replacement: display.clone(),
                    }]));
                }
                // Script produced no candidates: ring the bell, leave input
                // unchanged.
                let _ = std::io::stdout().write_all(b"\x07");
                let _ = std::io::stdout().flush();
                let display = partial.to_string();
                return Ok((word_start, vec![Pair {
                    display: display.clone(),
                    replacement: display.clone(),
                }]));
            }

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
