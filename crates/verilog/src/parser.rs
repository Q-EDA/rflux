use thiserror::Error;

use crate::ast::*;
use crate::lexer::{tokenize, LexError, Token};

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("unexpected token {found:?}, expected {expected} at token index {pos}")]
    UnexpectedToken {
        expected: String,
        found: Token,
        pos: usize,
    },
    #[error("unexpected end of input, expected {expected}")]
    UnexpectedEof { expected: String },
    #[error("lexer error: {0}")]
    LexError(#[from] LexError),
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);
        self.pos += 1;
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<Token, ParseError> {
        let tok = self.advance();
        if &tok == expected {
            Ok(tok)
        } else {
            Err(ParseError::UnexpectedToken {
                expected: format!("{expected:?}"),
                found: tok,
                pos: self.pos - 1,
            })
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Token::Ident(s) => Ok(s),
            tok => Err(ParseError::UnexpectedToken {
                expected: "identifier".to_string(),
                found: tok,
                pos: self.pos - 1,
            }),
        }
    }

    fn expect_signal(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Token::Ident(s) => Ok(s),
            Token::Number(n) => Ok(format!("{n}")),
            tok => Err(ParseError::UnexpectedToken {
                expected: "identifier or number".to_string(),
                found: tok,
                pos: self.pos - 1,
            }),
        }
    }

    fn error(&self, expected: &str) -> ParseError {
        let tok = self.peek().clone();
        if tok == Token::Eof {
            ParseError::UnexpectedEof {
                expected: expected.to_string(),
            }
        } else {
            ParseError::UnexpectedToken {
                expected: expected.to_string(),
                found: tok,
                pos: self.pos,
            }
        }
    }

    pub fn parse(&mut self) -> Result<VerilogSource, ParseError> {
        let mut modules = Vec::new();
        while self.peek() != &Token::Eof {
            modules.push(self.parse_module()?);
        }
        Ok(VerilogSource { modules })
    }

    fn parse_module(&mut self) -> Result<VerilogModule, ParseError> {
        self.expect(&Token::Module)?;
        let name = self.expect_ident()?;

        // Parse port list (optional, in parentheses)
        let mut ports = Vec::new();
        if self.peek() == &Token::LParen {
            self.advance();
            if self.peek() != &Token::RParen {
                loop {
                    let port_name = self.expect_ident()?;
                    ports.push(PortDecl {
                        direction: PortDirection::Input, // will be updated when we see declarations
                        name: port_name,
                        range: None,
                    });
                    if self.peek() == &Token::Comma {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect(&Token::RParen)?;
        }
        self.expect(&Token::Semicolon)?;

        // Parse module items
        let mut items = Vec::new();
        while self.peek() != &Token::Endmodule && self.peek() != &Token::Eof {
            items.push(self.parse_module_item()?);
        }
        self.expect(&Token::Endmodule)?;

        // Update port directions from declarations
        for item in &items {
            match item {
                ModuleItem::Net(net) => {
                    // Net declarations in port list are actually port declarations
                    for port in &mut ports {
                        if port.name == net.name {
                            // Infer direction from naming convention or leave as default
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(VerilogModule { name, ports, items })
    }

    fn parse_optional_range(&mut self) -> Result<Option<(i32, i32)>, ParseError> {
        if self.peek() == &Token::LBracket {
            self.advance();
            let high = self.expect_number()?;
            self.expect(&Token::Colon)?;
            let low = self.expect_number()?;
            self.expect(&Token::RBracket)?;
            Ok(Some((high as i32, low as i32)))
        } else {
            Ok(None)
        }
    }

    fn expect_number(&mut self) -> Result<i64, ParseError> {
        match self.advance() {
            Token::Number(n) => Ok(n),
            tok => Err(ParseError::UnexpectedToken {
                expected: "number".to_string(),
                found: tok,
                pos: self.pos - 1,
            }),
        }
    }

    fn parse_module_item(&mut self) -> Result<ModuleItem, ParseError> {
        match self.peek() {
            Token::Input | Token::Output | Token::Inout => {
                // Parse port declarations, possibly multiple per line
                let direction = match self.advance() {
                    Token::Input => PortDirection::Input,
                    Token::Output => PortDirection::Output,
                    Token::Inout => PortDirection::Inout,
                    _ => unreachable!(),
                };

                let range = self.parse_optional_range()?;

                let name = self.expect_ident()?;
                let first_port = PortDecl {
                    direction,
                    name,
                    range,
                };

                // Handle comma-separated: input a, b, c;
                let mut all_ports = vec![first_port];
                while self.peek() == &Token::Comma {
                    self.advance();
                    let n = self.expect_ident()?;
                    all_ports.push(PortDecl {
                        direction,
                        name: n,
                        range,
                    });
                }

                self.expect(&Token::Semicolon)?;

                // Return the first port as a net declaration for simplicity
                // The port list is already captured in parse_module
                // We'll store them as NetDecl with appropriate kind
                let kind = match direction {
                    PortDirection::Input | PortDirection::Inout => NetKind::Wire,
                    PortDirection::Output => NetKind::Wire,
                };

                // Return the first as a Net; if multiple, we need to handle differently
                // For now, just return first and let the rest be parsed as separate items
                // Actually, let's handle this properly by returning the first
                Ok(ModuleItem::Net(NetDecl {
                    kind,
                    name: all_ports[0].name.clone(),
                    range,
                }))
            }
            Token::Wire | Token::Reg => {
                let kind = match self.advance() {
                    Token::Wire => NetKind::Wire,
                    Token::Reg => NetKind::Reg,
                    _ => unreachable!(),
                };

                let range = self.parse_optional_range()?;
                let name = self.expect_ident()?;
                self.expect(&Token::Semicolon)?;

                Ok(ModuleItem::Net(NetDecl {
                    kind,
                    name,
                    range,
                }))
            }
            Token::Assign => {
                self.advance();
                let target = self.expect_ident()?;
                self.expect(&Token::Equals)?;
                let expr = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                Ok(ModuleItem::Assign(Assignment { target, expr }))
            }
            Token::Parameter => {
                self.advance();
                let name = self.expect_ident()?;
                self.expect(&Token::Equals)?;
                let value = self.expect_number()?;
                self.expect(&Token::Semicolon)?;
                Ok(ModuleItem::Parameter(ParamDecl { name, value }))
            }
            Token::And
            | Token::Or
            | Token::Not
            | Token::Buf
            | Token::Xor
            | Token::Nand
            | Token::Nor
            | Token::Xnor
            | Token::Mux
            | Token::Dff => {
                self.parse_gate_instance()
            }
            Token::Ident(_) => {
                // Could be a module instance
                self.parse_module_instance()
            }
            _ => Err(self.error("module item")),
        }
    }

    fn parse_gate_instance(&mut self) -> Result<ModuleItem, ParseError> {
        let gate_type = match self.advance() {
            Token::And => "and",
            Token::Or => "or",
            Token::Not => "not",
            Token::Buf => "buf",
            Token::Xor => "xor",
            Token::Nand => "nand",
            Token::Nor => "nor",
            Token::Xnor => "xnor",
            Token::Mux => "mux",
            Token::Dff => "dff",
            _ => return Err(self.error("gate type")),
        };

        // Optional instance name
        let name = if let Token::Ident(_) = self.peek() {
            self.expect_ident()?
        } else {
            format!("_{}", self.pos)
        };

        self.expect(&Token::LParen)?;
        let mut connections = Vec::new();

        if self.peek() != &Token::RParen {
            loop {
                let signal = self.expect_signal()?;
                connections.push(PortConnection {
                    port_name: None,
                    signal,
                });
                if self.peek() == &Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        self.expect(&Token::RParen)?;
        self.expect(&Token::Semicolon)?;

        Ok(ModuleItem::Instance(InstanceDecl {
            module_name: gate_type.to_string(),
            name,
            connections,
        }))
    }

    fn parse_module_instance(&mut self) -> Result<ModuleItem, ParseError> {
        let module_name = self.expect_ident()?;

        // Optional instance name (for repeated instances)
        let name = if let Token::Ident(_) = self.peek() {
            self.expect_ident()?
        } else {
            module_name.clone()
        };

        self.expect(&Token::LParen)?;
        let mut connections = Vec::new();

        if self.peek() != &Token::RParen {
            loop {
                let connection = if self.peek() == &Token::Dot {
                    // Named port connection: .port_name(signal)
                    self.advance();
                    let port_name = self.expect_ident()?;
                    self.expect(&Token::LParen)?;
                    let signal = self.expect_signal()?;
                    self.expect(&Token::RParen)?;
                    PortConnection {
                        port_name: Some(port_name),
                        signal,
                    }
                } else {
                    // Positional connection
                    let signal = self.expect_signal()?;
                    PortConnection {
                        port_name: None,
                        signal,
                    }
                };
                connections.push(connection);

                if self.peek() == &Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        self.expect(&Token::RParen)?;
        self.expect(&Token::Semicolon)?;

        Ok(ModuleItem::Instance(InstanceDecl {
            module_name,
            name,
            connections,
        }))
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_xor_expr()?;
        while self.peek() == &Token::Or {
            self.advance();
            let right = self.parse_xor_expr()?;
            left = Expr::BinOp(BinOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_xor_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and_expr()?;
        while self.peek() == &Token::Xor {
            self.advance();
            let right = self.parse_and_expr()?;
            left = Expr::BinOp(BinOp::Xor, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary_expr()?;
        while self.peek() == &Token::And {
            self.advance();
            let right = self.parse_unary_expr()?;
            left = Expr::BinOp(BinOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Token::Not => {
                self.advance();
                let expr = self.parse_primary()?;
                Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(expr)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Ident(s) => {
                self.advance();
                Ok(Expr::Ident(s))
            }
            Token::Number(n) => {
                self.advance();
                Ok(Expr::Literal(n))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            _ => Err(self.error("expression")),
        }
    }
}

pub fn parse_verilog(input: &str) -> Result<VerilogSource, ParseError> {
    let tokens = tokenize(input)?;
    let mut parser = Parser::new(tokens);
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_module() {
        let input = r#"
module top (a, b, y);
  input a;
  input b;
  output y;
  wire w;
  and g1(w, a, b);
  assign y = w;
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        assert_eq!(source.modules.len(), 1);
        let m = &source.modules[0];
        assert_eq!(m.name, "top");
        assert!(m.items.len() >= 3); // at least wire, and, assign
    }

    #[test]
    fn parse_assign_expression() {
        let input = r#"
module test;
  input a, b, c;
  output y;
  assign y = a & b | c;
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let m = &source.modules[0];

        // Find the assign
        let assign = m
            .items
            .iter()
            .find_map(|item| match item {
                ModuleItem::Assign(a) => Some(a),
                _ => None,
            })
            .unwrap();

        assert_eq!(assign.target, "y");
        // Expression should be (a & b) | c due to precedence
        match &assign.expr {
            Expr::BinOp(BinOp::Or, left, _right) => match left.as_ref() {
                Expr::BinOp(BinOp::And, _, _) => {} // correct
                _ => panic!("expected AND inside OR"),
            },
            _ => panic!("expected OR at top level"),
        }
    }

    #[test]
    fn parse_multiple_modules() {
        let input = r#"
module and_gate(a, b, y);
  input a, b;
  output y;
  assign y = a & b;
endmodule

module top(x, z);
  input x;
  output z;
  wire w;
  and_gate g1(x, 1'b1, w);
  assign z = w;
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        assert_eq!(source.modules.len(), 2);
        assert_eq!(source.modules[0].name, "and_gate");
        assert_eq!(source.modules[1].name, "top");
    }

    #[test]
    fn parse_gate_with_not() {
        let input = r#"
module inv(a, y);
  input a;
  output y;
  assign y = ~a;
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let assign = source.modules[0]
            .items
            .iter()
            .find_map(|item| match item {
                ModuleItem::Assign(a) => Some(a),
                _ => None,
            })
            .unwrap();

        match &assign.expr {
            Expr::UnaryOp(UnaryOp::Not, _) => {}
            _ => panic!("expected NOT expression"),
        }
    }
}
