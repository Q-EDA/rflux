use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    // Keywords
    Module,
    Endmodule,
    Input,
    Output,
    Inout,
    Wire,
    Reg,
    Assign,
    Parameter,
    Defparam,
    // Gate types
    And,
    Or,
    Not,
    Buf,
    Xor,
    Nand,
    Nor,
    Xnor,
    Mux,
    Dff,
    // Symbols
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Comma,
    Dot,
    Colon,
    Equals,
    At,
    Hash,
    // Literals and identifiers
    Ident(String),
    Number(i64),
    // Special
    Eof,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LexError {
    #[error("unexpected character '{0}' at line {1}, col {2}")]
    UnexpectedChar(char, usize, usize),
    #[error("unterminated block comment starting at line {0}")]
    UnterminatedComment(usize),
    #[error("invalid number format at line {0}, col {1}")]
    InvalidNumber(usize, usize),
}

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied()?;
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.advance() {
            if ch == '\n' {
                break;
            }
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        let start_line = self.line;
        loop {
            match self.advance() {
                Some('*') => {
                    if self.peek() == Some('/') {
                        self.advance();
                        return Ok(());
                    }
                }
                None => return Err(LexError::UnterminatedComment(start_line)),
                _ => {}
            }
        }
    }

    fn read_ident(&mut self) -> String {
        let start = self.pos - 1; // already advanced past first char
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                self.advance();
            } else {
                break;
            }
        }
        self.input[start..self.pos].iter().collect()
    }

    fn read_number(&mut self, first_digit: Option<char>) -> Result<i64, LexError> {
        // Check for sized number format: <size>'<base><value>
        // We may have already consumed some digits before realizing this is a number
        let mut num_str = String::new();
        if let Some(d) = first_digit {
            num_str.push(d);
        }

        // Read digits (could be size prefix or plain decimal)
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        // Check for sized number format: <size>'b/h/d<value>
        if self.peek() == Some('\'') {
            self.advance(); // consume '\''

            let base_char = self.advance().ok_or(LexError::InvalidNumber(self.line, self.col))?;
            let base = match base_char.to_ascii_lowercase() {
                'b' => 2,
                'h' => 16,
                'd' => 10,
                'o' => 8,
                _ => return Err(LexError::InvalidNumber(self.line, self.col)),
            };

            let mut value_str = String::new();
            while let Some(ch) = self.peek() {
                if ch == '_' {
                    self.advance();
                    continue;
                }
                if ch.is_alphanumeric() {
                    value_str.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }

            i64::from_str_radix(&value_str, base)
                .map_err(|_| LexError::InvalidNumber(self.line, self.col))
        } else {
            // Plain decimal number
            num_str
                .parse::<i64>()
                .map_err(|_| LexError::InvalidNumber(self.line, self.col))
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();

        loop {
            self.skip_whitespace();

            let ch = match self.peek() {
                Some(ch) => ch,
                None => {
                    tokens.push(Token::Eof);
                    return Ok(tokens);
                }
            };

            match ch {
                '/' => {
                    self.advance();
                    match self.peek() {
                        Some('/') => {
                            self.advance();
                            self.skip_line_comment();
                        }
                        Some('*') => {
                            self.advance();
                            self.skip_block_comment()?;
                        }
                        _ => {
                            return Err(LexError::UnexpectedChar('/', self.line, self.col - 1));
                        }
                    }
                }
                '(' => {
                    self.advance();
                    tokens.push(Token::LParen);
                }
                ')' => {
                    self.advance();
                    tokens.push(Token::RParen);
                }
                '{' => {
                    self.advance();
                    tokens.push(Token::LBrace);
                }
                '}' => {
                    self.advance();
                    tokens.push(Token::RBrace);
                }
                '[' => {
                    self.advance();
                    tokens.push(Token::LBracket);
                }
                ']' => {
                    self.advance();
                    tokens.push(Token::RBracket);
                }
                ';' => {
                    self.advance();
                    tokens.push(Token::Semicolon);
                }
                ',' => {
                    self.advance();
                    tokens.push(Token::Comma);
                }
                '.' => {
                    self.advance();
                    tokens.push(Token::Dot);
                }
                ':' => {
                    self.advance();
                    tokens.push(Token::Colon);
                }
                '=' => {
                    self.advance();
                    tokens.push(Token::Equals);
                }
                '@' => {
                    self.advance();
                    tokens.push(Token::At);
                }
                '#' => {
                    self.advance();
                    tokens.push(Token::Hash);
                }
                '&' => {
                    self.advance();
                    tokens.push(Token::And); // bitwise and, also used as AND gate keyword in some contexts
                }
                '|' => {
                    self.advance();
                    tokens.push(Token::Or); // bitwise or
                }
                '~' => {
                    self.advance();
                    tokens.push(Token::Not); // bitwise not
                }
                '^' => {
                    self.advance();
                    tokens.push(Token::Xor); // bitwise xor
                }
                '0'..='9' => {
                    self.advance();
                    let num = self.read_number(Some(ch))?;
                    tokens.push(Token::Number(num));
                }
                'a'..='z' | 'A'..='Z' | '_' | '\\' => {
                    self.advance();
                    let ident = self.read_ident();
                    let keyword = match ident.as_str() {
                        "module" => Token::Module,
                        "endmodule" => Token::Endmodule,
                        "input" => Token::Input,
                        "output" => Token::Output,
                        "inout" => Token::Inout,
                        "wire" => Token::Wire,
                        "reg" => Token::Reg,
                        "assign" => Token::Assign,
                        "parameter" => Token::Parameter,
                        "defparam" => Token::Defparam,
                        "and" => Token::And,
                        "or" => Token::Or,
                        "not" => Token::Not,
                        "buf" => Token::Buf,
                        "xor" => Token::Xor,
                        "nand" => Token::Nand,
                        "nor" => Token::Nor,
                        "xnor" => Token::Xnor,
                        "mux" => Token::Mux,
                        "dff" => Token::Dff,
                        _ => Token::Ident(ident),
                    };
                    tokens.push(keyword);
                }
                _ => {
                    return Err(LexError::UnexpectedChar(ch, self.line, self.col));
                }
            }
        }
    }
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(input).tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_simple_module() {
        let input = r#"
module top (a, b, y);
  input a, b;
  output y;
  wire w;
  and g1(w, a, b);
  assign y = w;
endmodule
"#;
        let tokens = tokenize(input).unwrap();
        assert!(tokens.contains(&Token::Module));
        assert!(tokens.contains(&Token::Endmodule));
        assert!(tokens.contains(&Token::Input));
        assert!(tokens.contains(&Token::Output));
        assert!(tokens.contains(&Token::Wire));
        assert!(tokens.contains(&Token::Assign));
        assert!(tokens.contains(&Token::And));
        assert!(tokens.contains(&Token::Ident("top".to_string())));
        assert!(tokens.contains(&Token::Ident("a".to_string())));
        assert!(tokens.contains(&Token::Ident("y".to_string())));
    }

    #[test]
    fn tokenize_gate_instance() {
        let input = "and g1(out, in1, in2);";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens[0], Token::And);
        assert_eq!(tokens[1], Token::Ident("g1".to_string()));
        assert_eq!(tokens[2], Token::LParen);
        assert_eq!(tokens[3], Token::Ident("out".to_string()));
        assert_eq!(tokens[4], Token::Comma);
        assert_eq!(tokens[5], Token::Ident("in1".to_string()));
        assert_eq!(tokens[6], Token::Comma);
        assert_eq!(tokens[7], Token::Ident("in2".to_string()));
        assert_eq!(tokens[8], Token::RParen);
        assert_eq!(tokens[9], Token::Semicolon);
    }

    #[test]
    fn tokenize_number_formats() {
        // Decimal
        let tokens = tokenize("42").unwrap();
        assert_eq!(tokens[0], Token::Number(42));

        // Binary
        let tokens = tokenize("8'b1010_0101").unwrap();
        assert_eq!(tokens[0], Token::Number(0b1010_0101));

        // Hex
        let tokens = tokenize("8'hFF").unwrap();
        assert_eq!(tokens[0], Token::Number(0xFF));

        // Sized decimal
        let tokens = tokenize("8'd255").unwrap();
        assert_eq!(tokens[0], Token::Number(255));
    }

    #[test]
    fn tokenize_comments() {
        let input = r#"
// line comment
module /* block comment */ top;
endmodule
"#;
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens[0], Token::Module);
        assert_eq!(tokens[1], Token::Ident("top".to_string()));
        assert_eq!(tokens[2], Token::Semicolon);
        assert_eq!(tokens[3], Token::Endmodule);
    }
}
