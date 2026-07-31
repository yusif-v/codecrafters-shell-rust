use std::fs::OpenOptions;
use std::io::Write;

/// Holds the stdout/stderr redirect targets parsed from a command line, along
/// with whether each is in append mode (`>>`/`1>>`/`2>>`).
pub struct Redirection {
    pub stdout: Option<String>,
    pub stdout_append: bool,
    pub stderr: Option<String>,
    pub stderr_append: bool,
}

/// Extracts redirection operators (`>`, `1>`, `2>`, `>>`, `1>>`, `2>>`) from a
/// token list.
///
/// Returns the tokens with each redirect operator and its filename removed,
/// plus the resolved targets. Only the FIRST occurrence of each operator is
/// honored. `<` is handled by a later stage.
pub fn parse_redirections(args: &[String]) -> (Vec<String>, Redirection) {
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
pub fn emit(text: &str, redirect: &Redirection) {
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
