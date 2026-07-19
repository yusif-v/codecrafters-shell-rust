use std::io::{self, BufRead, Write};

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
        // For now, every command is invalid.
        let command = input.split_whitespace().next().unwrap();
        println!("{}: command not found", command);
    }
}
