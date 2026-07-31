/// Splits a command line into arguments, honoring single and double quotes.
///
/// Whitespace outside quotes delimits arguments. Inside quotes (single or
/// double) every character is literal for this stage: spaces are preserved and
/// other quote characters lose their special meaning (so a ' inside "..." and a
/// " inside '...' are literal). Adjacent quoted/unquoted segments concatenate
/// into one argument; empty quotes ('') contribute nothing. (Later stages will
/// add $ / \ interpretation inside double quotes.)
pub fn tokenize(input: &str) -> Vec<String> {
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
