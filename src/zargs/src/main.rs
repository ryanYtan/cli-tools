use std::io::{self, BufRead};
use std::process::Command;

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

/// A minimal cursor over a string's characters, mirroring the reference
/// implementation's `Stream` helper used to hand-roll the `{n}` parser.
struct Stream {
    chars: Vec<char>,
    i: isize,
}

impl Stream {
    fn new(s: &str) -> Self {
        Self {
            chars: s.chars().collect(),
            i: 0,
        }
    }

    fn look(&self, offset: isize) -> Option<char> {
        let t = self.i + offset;
        if t < 0 || t as usize >= self.chars.len() {
            None
        } else {
            Some(self.chars[t as usize])
        }
    }

    fn consume(&mut self, n: isize) {
        self.i += n;
        if self.i > self.chars.len() as isize {
            self.i = self.chars.len() as isize;
        }
        if self.i < 0 {
            self.i = -1;
        }
    }

    fn is_at_end(&self) -> bool {
        self.i < 0 || self.i as usize >= self.chars.len()
    }

    fn pos(&self) -> isize {
        self.i
    }

    fn skip_spaces(&mut self) -> usize {
        let mut skipped = 0;
        while let Some(c) = self.look(0) {
            if !c.is_whitespace() {
                break;
            }
            skipped += 1;
            self.consume(1);
        }
        skipped
    }
}

/// Parses a command-template argument into literal and `{n}` positional
/// tokens. `{{` and `}}` escape to literal braces.
fn parse_fmt(s: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut st = Stream::new(s);

    while !st.is_at_end() {
        let skipped = st.skip_spaces();
        if skipped > 0 {
            tokens.push(Token::Literal(" ".repeat(skipped)));
        } else if st.look(0) == Some('{') && st.look(1) != Some('{') {
            st.consume(1);
            if st.is_at_end() {
                return Err(format!("Expected '}}' in {s} at position {}", st.pos()));
            }
            let mut pos_str = String::new();
            while let Some(c) = st.look(0) {
                if c.is_ascii_digit() {
                    pos_str.push(c);
                    st.consume(1);
                } else {
                    break;
                }
            }
            if st.look(0) != Some('}') {
                return Err(format!("Expected '}}' in {s} at position {}", st.pos()));
            }
            st.consume(1);
            let pos: usize = pos_str
                .parse()
                .map_err(|_| format!("Expected a positional index in {{}} in {s}"))?;
            tokens.push(Token::Positional(pos));
        } else if st.look(0) == Some('}') {
            st.consume(1);
            if st.look(0) == Some('}') {
                st.consume(1);
                tokens.push(Token::Literal("}".to_string()));
            } else {
                return Err(format!("Unexpected '}}' in {s} at position {}", st.pos()));
            }
        } else {
            let mut curr = String::new();
            while !st.is_at_end() {
                if st.look(0) == Some('{') {
                    if st.look(1) == Some('{') {
                        curr.push('{');
                        st.consume(2);
                    } else {
                        break;
                    }
                } else if st.look(0) == Some('}') {
                    if st.look(1) == Some('}') {
                        curr.push('}');
                        st.consume(2);
                    } else {
                        return Err(format!("Unexpected '}}' in {s} at position {}", st.pos()));
                    }
                } else {
                    curr.push(st.look(0).unwrap());
                    st.consume(1);
                }
            }
            tokens.push(Token::Literal(curr));
        }
    }

    Ok(tokens)
}

/// A simpler xargs. Use positional arguments in the command with {0}, {1}
/// e.g. `echo "a b c" | zargs -- echo "{1} {0} {2}"`
#[derive(Parser)]
#[command(
    about = "A simpler xargs. Use positional arguments in the command with {0}, {1} e.g. `echo \"a b c\" | zargs -- echo \"{1} {0} {2}\"`"
)]
struct Args {
    /// Command to run
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

fn main() {
    let args = Args::parse();

    let formatter: Vec<Vec<Token>> = args
        .command
        .iter()
        .map(|c| {
            parse_fmt(c).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1);
            })
        })
        .collect();

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.expect("failed to read from stdin");
        let parts: Vec<String> = line.split(' ').map(|s| s.to_string()).collect();

        let cmd: Vec<String> = formatter
            .iter()
            .map(|f| f.iter().map(|t| t.substitute(&parts)).collect::<String>())
            .collect();

        if cmd.is_empty() || cmd[0].is_empty() {
            continue;
        }

        if let Err(e) = Command::new(&cmd[0]).args(&cmd[1..]).status() {
            eprintln!("failed to run {cmd:?}: {e}");
            std::process::exit(1);
        }
    }
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
