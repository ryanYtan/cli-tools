use std::io::{self, BufRead};
use std::process::{Command, ExitCode};

use clap::Parser;

#[derive(Debug)]
enum Token {
    Positional(usize),
    Literal(String),
}

impl Token {
    fn substitute(&self, args: &[String]) -> String {
        match self {
            Token::Positional(pos) => args.get(*pos).cloned().unwrap_or_default(),
            Token::Literal(s) => s.clone(),
        }
    }
}

/// Parses a command-template argument into literal and `{n}` positional
/// tokens. `{{` and `}}` escape to literal braces.
fn parse_fmt(s: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut tokens = Vec::new();
    let mut literal = String::new();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            c @ ('{' | '}') if chars.get(i + 1) == Some(&c) => {
                literal.push(c);
                i += 2;
            }
            '{' => {
                if !literal.is_empty() {
                    tokens.push(Token::Literal(std::mem::take(&mut literal)));
                }
                let start = i + 1;
                let digits = chars[start..].iter().take_while(|c| c.is_ascii_digit()).count();
                let end = start + digits;
                if chars.get(end) != Some(&'}') {
                    return Err(format!("Expected '}}' in {s} at position {end}"));
                }
                let pos = String::from_iter(&chars[start..end])
                    .parse()
                    .map_err(|_| format!("Expected a positional index in {{}} in {s}"))?;
                tokens.push(Token::Positional(pos));
                i = end + 1;
            }
            '}' => return Err(format!("Unexpected '}}' in {s} at position {i}")),
            c => {
                literal.push(c);
                i += 1;
            }
        }
    }
    if !literal.is_empty() {
        tokens.push(Token::Literal(literal));
    }

    Ok(tokens)
}

/// A simpler xargs. Use positional arguments in the command with {0}, {1}
/// e.g. `echo "a b c" | zargs -- echo "{1} {0} {2}"`
#[derive(Parser)]
struct Args {
    /// Command to run
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

fn main() -> ExitCode {
    common::run(env!("CARGO_PKG_NAME"), run)
}

fn run() -> common::Result {
    let args = Args::parse();

    let formatter: Vec<Vec<Token>> = args
        .command
        .iter()
        .map(|c| parse_fmt(c))
        .collect::<Result<_, _>>()?;

    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| format!("failed to read from stdin: {e}"))?;
        let parts: Vec<String> = line.split(' ').map(|s| s.to_string()).collect();

        let cmd: Vec<String> = formatter
            .iter()
            .map(|f| f.iter().map(|t| t.substitute(&parts)).collect::<String>())
            .collect();

        if cmd.is_empty() || cmd[0].is_empty() {
            continue;
        }

        Command::new(&cmd[0])
            .args(&cmd[1..])
            .status()
            .map_err(|e| format!("failed to run {cmd:?}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(template: &str, args: &[&str]) -> Result<String, String> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        Ok(parse_fmt(template)?
            .iter()
            .map(|t| t.substitute(&args))
            .collect())
    }

    #[test]
    fn literal_text_passes_through() {
        assert_eq!(render("hello world", &[]), Ok("hello world".to_string()));
    }

    #[test]
    fn substitutes_positional_arguments() {
        assert_eq!(
            render("{1}-{0}", &["a", "b"]),
            Ok("b-a".to_string())
        );
    }

    #[test]
    fn out_of_range_positional_substitutes_empty() {
        assert_eq!(render("{5}", &["a"]), Ok("".to_string()));
    }

    #[test]
    fn escaped_braces_become_literal_braces() {
        assert_eq!(render("{{0}}", &["a"]), Ok("{0}".to_string()));
        assert_eq!(render("{{}}", &[]), Ok("{}".to_string()));
    }

    #[test]
    fn mixes_literal_and_positional_text() {
        assert_eq!(
            render("val={0}!", &["x"]),
            Ok("val=x!".to_string())
        );
    }

    #[test]
    fn whitespace_runs_are_preserved() {
        assert_eq!(render("a   b", &[]), Ok("a   b".to_string()));
    }

    #[test]
    fn other_whitespace_is_preserved() {
        assert_eq!(render("a\t{0}\n", &["x"]), Ok("a\tx\n".to_string()));
    }

    #[test]
    fn braces_pair_up_left_to_right() {
        assert_eq!(render("{{{0}}}", &["a"]), Ok("{a}".to_string()));
        assert_eq!(render("{0}}}", &["a"]), Ok("a}".to_string()));
    }

    #[test]
    fn unterminated_brace_is_an_error() {
        assert!(render("{0", &[]).is_err());
        assert!(render("{", &[]).is_err());
    }

    #[test]
    fn non_numeric_positional_is_an_error() {
        assert!(render("{a}", &[]).is_err());
    }

    #[test]
    fn empty_braces_is_an_error() {
        assert!(render("{}", &[]).is_err());
    }

    #[test]
    fn unescaped_lone_closing_brace_is_an_error() {
        assert!(render("a}b", &[]).is_err());
    }
}
