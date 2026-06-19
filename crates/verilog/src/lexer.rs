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
    Always,
    If,
    Else,
    Case,
    Casex,
    Casez,
    Endcase,
    Begin,
    End,
    Posedge,
    Negedge,
    Default,
    // Gate types (also used as keywords)
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
    Question,
    // Arithmetic operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    // Comparison operators
    EqEq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    // Shift operators
    Shl,
    Shr,
    // Bitwise operators
    BitAnd,
    BitOr,
    BitXor,
    Tilde,
    // Logical operators
    LogicalAnd,
    LogicalOr,
    LogicalNot,
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

    fn peek2(&self) -> Option<char> {
        self.input.get(self.pos + 1).copied()
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
                    match self.peek2() {
                        Some('/') => {
                            self.advance();
                            self.advance();
                            self.skip_line_comment();
                        }
                        Some('*') => {
                            self.advance();
                            self.advance();
                            self.skip_block_comment()?;
                        }
                        _ => {
                            self.advance();
                            tokens.push(Token::Slash);
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
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token::EqEq);
                    } else {
                        tokens.push(Token::Equals);
                    }
                }
                '@' => {
                    self.advance();
                    tokens.push(Token::At);
                }
                '#' => {
                    self.advance();
                    tokens.push(Token::Hash);
                }
                '?' => {
                    self.advance();
                    tokens.push(Token::Question);
                }
                '+' => {
                    self.advance();
                    tokens.push(Token::Plus);
                }
                '-' => {
                    self.advance();
                    tokens.push(Token::Minus);
                }
                '*' => {
                    self.advance();
                    tokens.push(Token::Star);
                }
                '%' => {
                    self.advance();
                    tokens.push(Token::Percent);
                }
                '<' => {
                    self.advance();
                    match self.peek() {
                        Some('=') => {
                            self.advance();
                            tokens.push(Token::LtEq);
                        }
                        Some('<') => {
                            self.advance();
                            tokens.push(Token::Shl);
                        }
                        _ => {
                            tokens.push(Token::Lt);
                        }
                    }
                }
                '>' => {
                    self.advance();
                    match self.peek() {
                        Some('=') => {
                            self.advance();
                            tokens.push(Token::GtEq);
                        }
                        Some('>') => {
                            self.advance();
                            tokens.push(Token::Shr);
                        }
                        _ => {
                            tokens.push(Token::Gt);
                        }
                    }
                }
                '&' => {
                    self.advance();
                    if self.peek() == Some('&') {
                        self.advance();
                        tokens.push(Token::LogicalAnd);
                    } else {
                        tokens.push(Token::BitAnd);
                    }
                }
                '|' => {
                    self.advance();
                    if self.peek() == Some('|') {
                        self.advance();
                        tokens.push(Token::LogicalOr);
                    } else {
                        tokens.push(Token::BitOr);
                    }
                }
                '^' => {
                    self.advance();
                    tokens.push(Token::BitXor);
                }
                '~' => {
                    self.advance();
                    tokens.push(Token::Tilde);
                }
                '!' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token::NotEq);
                    } else {
                        tokens.push(Token::LogicalNot);
                    }
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
                        "always" => Token::Always,
                        "if" => Token::If,
                        "else" => Token::Else,
                        "case" => Token::Case,
                        "casex" => Token::Casex,
                        "casez" => Token::Casez,
                        "endcase" => Token::Endcase,
                        "begin" => Token::Begin,
                        "end" => Token::End,
                        "posedge" => Token::Posedge,
                        "negedge" => Token::Negedge,
                        "default" => Token::Default,
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

    #[test]
    fn tokenize_always_block() {
        let input = "always @(*) if (sel) y = a; else y = b;";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens[0], Token::Always);
        assert_eq!(tokens[1], Token::At);
        assert_eq!(tokens[2], Token::LParen);
        assert_eq!(tokens[3], Token::Star);
        assert_eq!(tokens[4], Token::RParen);
        assert_eq!(tokens[5], Token::If);
        assert_eq!(tokens[6], Token::LParen);
        assert_eq!(tokens[7], Token::Ident("sel".to_string()));
        assert_eq!(tokens[8], Token::RParen);
        assert_eq!(tokens[9], Token::Ident("y".to_string()));
        assert_eq!(tokens[10], Token::Equals);
        assert_eq!(tokens[11], Token::Ident("a".to_string()));
        assert_eq!(tokens[12], Token::Semicolon);
        assert_eq!(tokens[13], Token::Else);
        assert_eq!(tokens[14], Token::Ident("y".to_string()));
        assert_eq!(tokens[15], Token::Equals);
        assert_eq!(tokens[16], Token::Ident("b".to_string()));
        assert_eq!(tokens[17], Token::Semicolon);
    }

    #[test]
    fn tokenize_multi_char_operators() {
        let input = "a == b != c <= d >= e << f >> g && h || i !j";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens[0], Token::Ident("a".to_string()));
        assert_eq!(tokens[1], Token::EqEq);
        assert_eq!(tokens[2], Token::Ident("b".to_string()));
        assert_eq!(tokens[3], Token::NotEq);
        assert_eq!(tokens[4], Token::Ident("c".to_string()));
        assert_eq!(tokens[5], Token::LtEq);
        assert_eq!(tokens[6], Token::Ident("d".to_string()));
        assert_eq!(tokens[7], Token::GtEq);
        assert_eq!(tokens[8], Token::Ident("e".to_string()));
        assert_eq!(tokens[9], Token::Shl);
        assert_eq!(tokens[10], Token::Ident("f".to_string()));
        assert_eq!(tokens[11], Token::Shr);
        assert_eq!(tokens[12], Token::Ident("g".to_string()));
        assert_eq!(tokens[13], Token::LogicalAnd);
        assert_eq!(tokens[14], Token::Ident("h".to_string()));
        assert_eq!(tokens[15], Token::LogicalOr);
        assert_eq!(tokens[16], Token::Ident("i".to_string()));
        assert_eq!(tokens[17], Token::LogicalNot);
        assert_eq!(tokens[18], Token::Ident("j".to_string()));
    }

    #[test]
    fn tokenize_bitwise_operators() {
        let input = "a & b | c ^ d ~e";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens[0], Token::Ident("a".to_string()));
        assert_eq!(tokens[1], Token::BitAnd);
        assert_eq!(tokens[2], Token::Ident("b".to_string()));
        assert_eq!(tokens[3], Token::BitOr);
        assert_eq!(tokens[4], Token::Ident("c".to_string()));
        assert_eq!(tokens[5], Token::BitXor);
        assert_eq!(tokens[6], Token::Ident("d".to_string()));
        assert_eq!(tokens[7], Token::Tilde);
        assert_eq!(tokens[8], Token::Ident("e".to_string()));
    }

    #[test]
    fn tokenize_arithmetic_operators() {
        let input = "a + b - c * d / e % f";
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens[0], Token::Ident("a".to_string()));
        assert_eq!(tokens[1], Token::Plus);
        assert_eq!(tokens[2], Token::Ident("b".to_string()));
        assert_eq!(tokens[3], Token::Minus);
        assert_eq!(tokens[4], Token::Ident("c".to_string()));
        assert_eq!(tokens[5], Token::Star);
        assert_eq!(tokens[6], Token::Ident("d".to_string()));
        assert_eq!(tokens[7], Token::Slash);
        assert_eq!(tokens[8], Token::Ident("e".to_string()));
        assert_eq!(tokens[9], Token::Percent);
        assert_eq!(tokens[10], Token::Ident("f".to_string()));
    }
}
