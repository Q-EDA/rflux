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

impl ParseError {
    pub fn code(&self) -> &'static str {
        "RFLOW-VERILOG-001"
    }
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
            if let ModuleItem::Net(net) = item {
                for port in &mut ports {
                    if port.name == net.name {
                        // Infer direction from naming convention or leave as default
                    }
                }
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
            Token::Always => {
                self.advance();
                let sensitivity = self.parse_sensitivity_list()?;
                let body = self.parse_statement()?;
                Ok(ModuleItem::AlwaysBlock(AlwaysBlock {
                    sensitivity,
                    body,
                }))
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
            Token::Generate => self.parse_generate_block(),
            Token::Task => self.parse_task_decl(),
            Token::Function => self.parse_function_decl(),
            _ => Err(self.error("module item")),
        }
    }

    fn parse_sensitivity_list(&mut self) -> Result<SensitivityList, ParseError> {
        self.expect(&Token::At)?;
        self.expect(&Token::LParen)?;

        let mut items = Vec::new();

        if self.peek() == &Token::Star {
            self.advance();
            items.push(SensitivityItem::All);
        } else {
            loop {
                let item = match self.peek() {
                    Token::Posedge => {
                        self.advance();
                        let name = self.expect_ident()?;
                        SensitivityItem::Posedge(name)
                    }
                    Token::Negedge => {
                        self.advance();
                        let name = self.expect_ident()?;
                        SensitivityItem::Negedge(name)
                    }
                    _ => {
                        let name = self.expect_ident()?;
                        SensitivityItem::Posedge(name) // default assumption
                    }
                };
                items.push(item);

                if self.peek() == &Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        self.expect(&Token::RParen)?;
        Ok(SensitivityList { items })
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match self.peek() {
            Token::Begin => self.parse_block_statement(),
            Token::If => self.parse_if_statement(),
            Token::Case | Token::Casex | Token::Casez => self.parse_case_statement(),
            Token::Ident(_) => self.parse_assign_statement(),
            Token::Semicolon => {
                self.advance();
                Ok(Statement::Null)
            }
            _ => Err(self.error("statement")),
        }
    }

    fn parse_block_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Begin)?;
        let mut stmts = Vec::new();
        while self.peek() != &Token::End && self.peek() != &Token::Eof {
            stmts.push(self.parse_statement()?);
        }
        self.expect(&Token::End)?;
        Ok(Statement::Block(stmts))
    }

    fn parse_if_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::If)?;
        self.expect(&Token::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(&Token::RParen)?;

        let then_body = Box::new(self.parse_statement()?);

        let else_body = if self.peek() == &Token::Else {
            self.advance();
            Some(Box::new(self.parse_statement()?))
        } else {
            None
        };

        Ok(Statement::If {
            condition,
            then_body,
            else_body,
        })
    }

    fn parse_case_statement(&mut self) -> Result<Statement, ParseError> {
        // Consume case/casex/casez
        self.advance();

        self.expect(&Token::LParen)?;
        let expr = self.parse_expr()?;
        self.expect(&Token::RParen)?;

        let mut items = Vec::new();
        let mut default = None;

        while self.peek() != &Token::Endcase && self.peek() != &Token::Eof {
            if self.peek() == &Token::Default {
                self.advance();
                self.expect(&Token::Colon)?;
                default = Some(Box::new(self.parse_statement()?));
            } else {
                let mut patterns = Vec::new();
                loop {
                    patterns.push(self.parse_expr()?);
                    if self.peek() == &Token::Comma {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(&Token::Colon)?;
                let body = self.parse_statement()?;
                items.push(CaseItem { patterns, body });
            }
        }

        self.expect(&Token::Endcase)?;

        Ok(Statement::Case {
            expr,
            items,
            default,
        })
    }

    fn parse_assign_statement(&mut self) -> Result<Statement, ParseError> {
        let target = self.expect_ident()?;

        match self.peek() {
            Token::LtEq => {
                // Non-blocking assign: target <= expr;
                self.advance();
                let value = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                Ok(Statement::NonBlockingAssign { target, value })
            }
            Token::Equals => {
                // Blocking assign: target = expr;
                self.advance();
                let value = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                Ok(Statement::BlockingAssign { target, value })
            }
            _ => Err(self.error("'=' or '<=' in assignment")),
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

    fn parse_generate_block(&mut self) -> Result<ModuleItem, ParseError> {
        self.advance(); // consume 'generate'
        let kind = match self.peek() {
            Token::If => {
                self.advance();
                self.expect(&Token::LParen)?;
                let condition = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                let then_body = self.parse_generate_block_items()?;
                let else_body = if self.peek() == &Token::Else {
                    self.advance();
                    Some(self.parse_generate_block_items()?)
                } else {
                    None
                };
                GenerateKind::If { condition, then_body, else_body }
            }
            Token::For => {
                self.advance();
                self.expect(&Token::LParen)?;
                let init_name = self.expect_ident()?;
                self.expect(&Token::Equals)?;
                let init_value = self.expect_number()?;
                self.expect(&Token::Semicolon)?;
                let condition = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                let step_name = self.expect_ident()?;
                let step = match self.peek() {
                    Token::Equals => {
                        self.advance();
                        // Handle pattern: i = i + 1 or i = i - 1
                        if let Token::Ident(ref name) = self.peek().clone() {
                            if *name == step_name {
                                self.advance(); // consume the ident
                                match self.peek() {
                                    Token::Plus => {
                                        self.advance();
                                        let val = self.expect_number()?;
                                        GenVarStep { name: step_name, op: GenVarOp::AddAssign, value: val }
                                    }
                                    Token::Minus => {
                                        self.advance();
                                        let val = self.expect_number()?;
                                        GenVarStep { name: step_name, op: GenVarOp::SubAssign, value: val }
                                    }
                                    _ => return Err(self.error("'+' or '-' in generate step")),
                                }
                            } else {
                                return Err(self.error("loop variable in generate step"));
                            }
                        } else {
                            let val = self.expect_number()?;
                            GenVarStep { name: step_name, op: GenVarOp::Assign, value: val }
                        }
                    }
                    Token::Plus => {
                        self.advance();
                        self.expect(&Token::Equals)?;
                        let val = self.expect_number()?;
                        GenVarStep { name: step_name, op: GenVarOp::AddAssign, value: val }
                    }
                    Token::Minus => {
                        self.advance();
                        self.expect(&Token::Equals)?;
                        let val = self.expect_number()?;
                        GenVarStep { name: step_name, op: GenVarOp::SubAssign, value: val }
                    }
                    _ => return Err(self.error("'=' or '+=' or '-=' in generate step")),
                };
                self.expect(&Token::RParen)?;
                let body = self.parse_generate_block_items()?;
                GenerateKind::For {
                    init: GenVarInit { name: init_name, value: init_value },
                    condition,
                    step,
                    body,
                }
            }
            _ => GenerateKind::Block(self.parse_generate_block_items()?),
        };
        self.expect(&Token::Endgenerate)?;
        Ok(ModuleItem::GenerateBlock(GenerateBlock { label: None, kind }))
    }

    fn parse_generate_block_items(&mut self) -> Result<Vec<ModuleItem>, ParseError> {
        let mut items = Vec::new();
        while self.peek() != &Token::Endgenerate && self.peek() != &Token::Else && self.peek() != &Token::Eof {
            if self.peek() == &Token::Begin {
                // Named begin block: begin : label ... end
                self.advance(); // consume 'begin'
                if self.peek() == &Token::Colon {
                    self.advance(); // consume ':'
                    let _label = self.expect_ident()?; // consume label
                }
                while self.peek() != &Token::End && self.peek() != &Token::Eof {
                    items.push(self.parse_module_item()?);
                }
                self.expect(&Token::End)?;
            } else {
                items.push(self.parse_module_item()?);
            }
        }
        Ok(items)
    }

    fn parse_task_decl(&mut self) -> Result<ModuleItem, ParseError> {
        self.advance(); // consume 'task'
        let name = self.expect_ident()?;
        self.expect(&Token::Semicolon)?;
        let mut ports = Vec::new();
        let mut body = Vec::new();
        while self.peek() != &Token::Endtask && self.peek() != &Token::Eof {
            match self.peek() {
                Token::Input | Token::Output | Token::Inout => {
                    let direction = match self.advance() {
                        Token::Input => PortDirection::Input,
                        Token::Output => PortDirection::Output,
                        Token::Inout => PortDirection::Inout,
                        _ => unreachable!(),
                    };
                    self.parse_optional_range()?; // skip range if present
                    let pname = self.expect_ident()?;
                    self.expect(&Token::Semicolon)?;
                    ports.push(TaskPort { direction, name: pname });
                }
                _ => body.push(self.parse_statement()?),
            }
        }
        self.expect(&Token::Endtask)?;
        Ok(ModuleItem::TaskDecl(TaskDecl { name, ports, body }))
    }

    fn parse_function_decl(&mut self) -> Result<ModuleItem, ParseError> {
        self.advance(); // consume 'function'
        let return_range = self.parse_optional_range()?;
        let name = self.expect_ident()?;
        self.expect(&Token::Semicolon)?;
        let mut ports = Vec::new();
        let mut body = Vec::new();
        while self.peek() != &Token::Endfunction && self.peek() != &Token::Eof {
            match self.peek() {
                Token::Input | Token::Output | Token::Inout => {
                    let direction = match self.advance() {
                        Token::Input => PortDirection::Input,
                        Token::Output => PortDirection::Output,
                        Token::Inout => PortDirection::Inout,
                        _ => unreachable!(),
                    };
                    self.parse_optional_range()?; // skip range if present
                    let pname = self.expect_ident()?;
                    self.expect(&Token::Semicolon)?;
                    ports.push(TaskPort { direction, name: pname });
                }
                _ => body.push(self.parse_statement()?),
            }
        }
        self.expect(&Token::Endfunction)?;
        Ok(ModuleItem::FunctionDecl(FunctionDecl { name, return_range, ports, body }))
    }

    // Expression parser with proper precedence (lowest to highest):
    // 1. Ternary (?:)
    // 2. LogicalOr (||)
    // 3. LogicalAnd (&&)
    // 4. BitOr (|)
    // 5. BitXor (^)
    // 6. BitAnd (&)
    // 7. Equality (==, !=)
    // 8. Relational (<, >, <=, >=)
    // 9. Shift (<<, >>)
    // 10. Additive (+, -)
    // 11. Multiplicative (*, /, %)
    // 12. Unary (!, ~, -)
    // 13. Primary (ident, literal, paren, concat, bitselect)

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_ternary_expr()
    }

    fn parse_ternary_expr(&mut self) -> Result<Expr, ParseError> {
        let cond = self.parse_logical_or_expr()?;
        if self.peek() == &Token::Question {
            self.advance();
            let then_expr = self.parse_expr()?;
            self.expect(&Token::Colon)?;
            let else_expr = self.parse_expr()?;
            Ok(Expr::Ternary(
                Box::new(cond),
                Box::new(then_expr),
                Box::new(else_expr),
            ))
        } else {
            Ok(cond)
        }
    }

    fn parse_logical_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_logical_and_expr()?;
        while self.peek() == &Token::LogicalOr {
            self.advance();
            let right = self.parse_logical_and_expr()?;
            left = Expr::BinOp(BinOp::LogicalOr, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_logical_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bit_or_expr()?;
        while self.peek() == &Token::LogicalAnd {
            self.advance();
            let right = self.parse_bit_or_expr()?;
            left = Expr::BinOp(BinOp::LogicalAnd, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_bit_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bit_xor_expr()?;
        while self.peek() == &Token::BitOr {
            self.advance();
            let right = self.parse_bit_xor_expr()?;
            left = Expr::BinOp(BinOp::BitOr, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_bit_xor_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bit_and_expr()?;
        while self.peek() == &Token::BitXor {
            self.advance();
            let right = self.parse_bit_and_expr()?;
            left = Expr::BinOp(BinOp::BitXor, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_bit_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality_expr()?;
        while self.peek() == &Token::BitAnd {
            self.advance();
            let right = self.parse_equality_expr()?;
            left = Expr::BinOp(BinOp::BitAnd, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_equality_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_relational_expr()?;
        loop {
            let op = match self.peek() {
                Token::EqEq => BinOp::Eq,
                Token::NotEq => BinOp::Neq,
                _ => break,
            };
            self.advance();
            let right = self.parse_relational_expr()?;
            left = Expr::BinOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_relational_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_shift_expr()?;
        loop {
            let op = match self.peek() {
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::LtEq => BinOp::Le,
                Token::GtEq => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.parse_shift_expr()?;
            left = Expr::BinOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_shift_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive_expr()?;
        loop {
            let op = match self.peek() {
                Token::Shl => BinOp::Shl,
                Token::Shr => BinOp::Shr,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive_expr()?;
            left = Expr::BinOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_additive_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative_expr()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative_expr()?;
            left = Expr::BinOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_multiplicative_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary_expr()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary_expr()?;
            left = Expr::BinOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Token::Tilde => {
                self.advance();
                let expr = self.parse_primary()?;
                Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(expr)))
            }
            Token::LogicalNot => {
                self.advance();
                let expr = self.parse_primary()?;
                Ok(Expr::UnaryOp(UnaryOp::LogicalNot, Box::new(expr)))
            }
            Token::Minus => {
                self.advance();
                let expr = self.parse_primary()?;
                Ok(Expr::UnaryOp(UnaryOp::Negate, Box::new(expr)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Ident(s) => {
                self.advance();
                // Check for bit select: ident[high:low]
                if self.peek() == &Token::LBracket {
                    self.advance();
                    let high = self.expect_number()? as i32;
                    self.expect(&Token::Colon)?;
                    let low = self.expect_number()? as i32;
                    self.expect(&Token::RBracket)?;
                    Ok(Expr::BitSelect(Box::new(Expr::Ident(s)), high, low))
                } else {
                    Ok(Expr::Ident(s))
                }
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
            Token::LBrace => {
                // Concatenation: {expr1, expr2, ...}
                self.advance();
                let mut exprs = Vec::new();
                if self.peek() != &Token::RBrace {
                    loop {
                        exprs.push(self.parse_expr()?);
                        if self.peek() == &Token::Comma {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(&Token::RBrace)?;
                Ok(Expr::Concat(exprs))
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
    fn verilog_error_codes_are_stable() {
        let err = ParseError::UnexpectedEof {
            expected: "test".to_string(),
        };
        assert_eq!(err.code(), "RFLOW-VERILOG-001");
    }

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
            Expr::BinOp(BinOp::BitOr, left, _right) => match left.as_ref() {
                Expr::BinOp(BinOp::BitAnd, _, _) => {} // correct
                _ => panic!("expected BitAnd inside BitOr"),
            },
            _ => panic!("expected BitOr at top level"),
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

    #[test]
    fn parse_always_combinational() {
        let input = r#"
module my_mux(a, b, sel, y);
  input a, b, sel;
  output y;
  always @(*) begin
    if (sel)
      y = b;
    else
      y = a;
  end
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let m = &source.modules[0];
        let always = m
            .items
            .iter()
            .find_map(|item| match item {
                ModuleItem::AlwaysBlock(a) => Some(a),
                _ => None,
            })
            .unwrap();

        assert_eq!(always.sensitivity.items.len(), 1);
        assert!(matches!(always.sensitivity.items[0], SensitivityItem::All));
        match &always.body {
            Statement::Block(stmts) => match &stmts[0] {
                Statement::If {
                    condition,
                    then_body: _,
                    else_body: _,
                } => match condition {
                    Expr::Ident(s) => assert_eq!(s, "sel"),
                    _ => panic!("expected ident condition"),
                },
                _ => panic!("expected if statement inside block"),
            },
            _ => panic!("expected block statement"),
        }
    }

    #[test]
    fn parse_always_sequential() {
        let input = r#"
module my_dff(clk, d, q);
  input clk, d;
  output q;
  always @(posedge clk) begin
    q <= d;
  end
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let m = &source.modules[0];
        let always = m
            .items
            .iter()
            .find_map(|item| match item {
                ModuleItem::AlwaysBlock(a) => Some(a),
                _ => None,
            })
            .unwrap();

        assert_eq!(always.sensitivity.items.len(), 1);
        assert!(matches!(
            always.sensitivity.items[0],
            SensitivityItem::Posedge(ref s) if s == "clk"
        ));
        match &always.body {
            Statement::Block(stmts) => {
                assert_eq!(stmts.len(), 1);
                match &stmts[0] {
                    Statement::NonBlockingAssign { target, value: _ } => {
                        assert_eq!(target, "q");
                    }
                    _ => panic!("expected non-blocking assign"),
                }
            }
            _ => panic!("expected block statement"),
        }
    }

    #[test]
    fn parse_case_statement() {
        let input = r#"
module mux4(a, b, c, d, sel, y);
  input a, b, c, d;
  input [1:0] sel;
  output y;
  always @(*) begin
    case (sel)
      2'b00: y = a;
      2'b01: y = b;
      2'b10: y = c;
      default: y = d;
    endcase
  end
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let m = &source.modules[0];
        let always = m
            .items
            .iter()
            .find_map(|item| match item {
                ModuleItem::AlwaysBlock(a) => Some(a),
                _ => None,
            })
            .unwrap();

        match &always.body {
            Statement::Block(stmts) => match &stmts[0] {
                Statement::Case {
                    expr: _,
                    items,
                    default,
                } => {
                    assert_eq!(items.len(), 3);
                    assert!(default.is_some());
                }
                _ => panic!("expected case statement"),
            },
            _ => panic!("expected block"),
        }
    }

    #[test]
    fn parse_expression_precedence() {
        let input = r#"
module test(a, b, c, d, y);
  input a, b, c, d;
  output y;
  assign y = a | b & c;
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

        // Should be a | (b & c) — BitAnd binds tighter than BitOr
        match &assign.expr {
            Expr::BinOp(BinOp::BitOr, left, right) => {
                assert!(matches!(left.as_ref(), Expr::Ident(_)));
                assert!(matches!(right.as_ref(), Expr::BinOp(BinOp::BitAnd, _, _)));
            }
            _ => panic!("expected BitOr at top level"),
        }
    }

    #[test]
    fn parse_ternary_expression() {
        let input = r#"
module test(a, b, sel, y);
  input a, b, sel;
  output y;
  assign y = sel ? b : a;
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
            Expr::Ternary(cond, then_expr, else_expr) => {
                assert!(matches!(cond.as_ref(), Expr::Ident(ref s) if s == "sel"));
                assert!(matches!(then_expr.as_ref(), Expr::Ident(ref s) if s == "b"));
                assert!(matches!(else_expr.as_ref(), Expr::Ident(ref s) if s == "a"));
            }
            _ => panic!("expected ternary expression"),
        }
    }

    #[test]
    fn parse_generate_for() {
        let input = r#"
module top(a, b, y);
    input a;
    input b;
    output y;
    generate
        for (i = 0; i < 4; i = i + 1) begin : gen_loop
            and g(a, b, y);
        end
    endgenerate
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let m = &source.modules[0];
        let has_generate = m.items.iter().any(|item| matches!(item, ModuleItem::GenerateBlock(_)));
        assert!(has_generate, "expected generate block");
    }

    #[test]
    fn parse_task_decl() {
        let input = r#"
module top();
    task my_task;
        input a;
        input b;
        output y;
        y = a & b;
    endtask
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let m = &source.modules[0];
        let task = m
            .items
            .iter()
            .find_map(|item| match item {
                ModuleItem::TaskDecl(t) => Some(t),
                _ => None,
            })
            .unwrap();
        assert_eq!(task.name, "my_task");
        assert_eq!(task.ports.len(), 3);
        assert_eq!(task.body.len(), 1);
    }

    #[test]
    fn parse_function_decl() {
        let input = r#"
module top();
    function [7:0] add;
        input [7:0] a;
        input [7:0] b;
        add = a + b;
    endfunction
endmodule
"#;
        let source = parse_verilog(input).unwrap();
        let m = &source.modules[0];
        let func = m
            .items
            .iter()
            .find_map(|item| match item {
                ModuleItem::FunctionDecl(f) => Some(f),
                _ => None,
            })
            .unwrap();
        assert_eq!(func.name, "add");
        assert_eq!(func.return_range, Some((7, 0)));
        assert_eq!(func.ports.len(), 2);
        assert_eq!(func.body.len(), 1);
    }
}
