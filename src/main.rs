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
        let mut parts = input.split_whitespace();
        let command = parts.next().unwrap();

        // Builtins are handled directly by the shell.
        match command {
            "exit" => break,
            "echo" => {
                // Print the remaining arguments joined by single spaces.
                let args: Vec<&str> = parts.collect();
                println!("{}", args.join(" "));
            }
            // For now, every other command is invalid.
            _ => println!("{}: command not found", command),
        }
    }
}
