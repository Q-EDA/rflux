# Verilog 前端 Phase 1 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 rflux 添加结构化 Verilog 解析能力作为输入格式

**Architecture:** 新增 `crates/verilog/` crate，手写递归下降解析器，输出 AST → 转换为 rflux IR Netlist。

**Tech Stack:** Rust, rflux-ir, rflux-io

---

## 文件结构

- Create: `crates/verilog/Cargo.toml`
- Create: `crates/verilog/src/lexer.rs`
- Create: `crates/verilog/src/parser.rs`
- Create: `crates/verilog/src/ast.rs`
- Create: `crates/verilog/src/elaborate.rs`
- Create: `crates/verilog/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)
- Modify: `crates/io/Cargo.toml`
- Modify: `crates/io/src/lib.rs` (add Verilog format)
- Modify: `crates/cli/Cargo.toml`
- Modify: `crates/cli/src/main.rs` (add --input-format verilog)

---

## Task 1: 创建 verilog crate 骨架和 AST

**Covers:** [S3]

**Files:**
- Create: `crates/verilog/Cargo.toml`
- Create: `crates/verilog/src/lib.rs`
- Create: `crates/verilog/src/ast.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: workspace 添加 verilog crate**

在 `Cargo.toml` 的 `members` 中添加 `"crates/verilog"`。

- [ ] **Step 2: 创建 Cargo.toml**

```toml
[package]
name = "rflux-verilog"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rflux-ir = { path = "../ir" }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror.workspace = true
```

- [ ] **Step 3: 创建 src/ast.rs**

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct VerilogSource {
    pub modules: Vec<VerilogModule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerilogModule {
    pub name: String,
    pub ports: Vec<PortDecl>,
    pub items: Vec<ModuleItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortDecl {
    pub direction: PortDirection,
    pub name: String,
    pub range: Option<(i32, i32)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PortDirection {
    Input,
    Output,
    Inout,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModuleItem {
    Net(NetDecl),
    Instance(InstanceDecl),
    Assign(Assignment),
    Parameter(ParamDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetDecl {
    pub kind: NetKind,
    pub name: String,
    pub range: Option<(i32, i32)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NetKind {
    Wire,
    Reg,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstanceDecl {
    pub module_name: String,
    pub name: String,
    pub connections: Vec<PortConnection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortConnection {
    pub port_name: Option<String>,
    pub signal: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub target: String,
    pub expr: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParamDecl {
    pub name: String,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Ident(String),
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),
    Literal(i64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    And,
    Or,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Not,
}
```

- [ ] **Step 4: 创建 src/lib.rs**

```rust
pub mod ast;
pub mod elaborate;
pub mod lexer;
pub mod parser;

pub use ast::*;
pub use elaborate::elaborate_to_ir;
pub use parser::parse_verilog;
```

- [ ] **Step 5: 验证编译**

Run: `cargo check -p rflux-verilog`

---

## Task 2: 实现 Lexer

**Covers:** [S5]

**Files:**
- Create: `crates/verilog/src/lexer.rs`

- [ ] **Step 1: 实现 Token 类型和 Lexer**

```rust
#[derive(Debug, Clone, PartialEq)]
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
    // Gate primitives
    And, Or, Not, Buf, Xor, Nand, Nor, Xnor, Mux, Dff,
    // Symbols
    LParen, RParen,
    LBracket, RBracket,
    Semicolon,
    Comma,
    Dot,
    Equals,
    Colon,
    At,
    Hash,
    Amp,      // &
    Pipe,     // |
    Caret,    // ^
    Tilde,    // ~
    // Literals and identifiers
    Ident(String),
    Number(i64),
    // Special
    Eof,
}

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub line: usize,
    pub col: usize,
    pub message: String,
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

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.input.len() {
                tokens.push(Token::Eof);
                break;
            }
            let token = self.next_token()?;
            tokens.push(token);
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<Token, LexError> {
        let ch = self.input[self.pos];
        match ch {
            '(' => { self.advance(); Ok(Token::LParen) }
            ')' => { self.advance(); Ok(Token::RParen) }
            '[' => { self.advance(); Ok(Token::LBracket) }
            ']' => { self.advance(); Ok(Token::RBracket) }
            ';' => { self.advance(); Ok(Token::Semicolon) }
            ',' => { self.advance(); Ok(Token::Comma) }
            '.' => { self.advance(); Ok(Token::Dot) }
            '=' => { self.advance(); Ok(Token::Equals) }
            ':' => { self.advance(); Ok(Token::Colon) }
            '@' => { self.advance(); Ok(Token::At) }
            '#' => { self.advance(); Ok(Token::Hash) }
            '&' => { self.advance(); Ok(Token::Amp) }
            '|' => { self.advance(); Ok(Token::Pipe) }
            '^' => { self.advance(); Ok(Token::Caret) }
            '~' => { self.advance(); Ok(Token::Tilde) }
            '\'' => self.read_sized_number(),
            '0'..='9' => self.read_number(),
            'a'..='z' | 'A'..='Z' | '_' | '\\' => self.read_identifier_or_keyword(),
            _ => Err(LexError {
                line: self.line,
                col: self.col,
                message: format!("unexpected character '{}'", ch),
            }),
        }
    }

    // Implement: advance(), peek(), skip_whitespace_and_comments(),
    // read_number(), read_sized_number(), read_identifier_or_keyword()
    // ...
}
```

需要实现完整的 lexer 方法。关键方法：
- `advance()` - 推进一个字符
- `peek()` - 预览下一个字符
- `skip_whitespace_and_comments()` - 跳过空格、换行、// 和 /* */ 注释
- `read_number()` - 读取数字（支持 'b, 'h, 'd 前缀）
- `read_identifier_or_keyword()` - 读取标识符，检查是否为关键字

- [ ] **Step 2: 添加 lexer 单元测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_simple_module() {
        let input = "module top(input a, output y); endmodule";
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0], Token::Module);
        assert_eq!(tokens[1], Token::Ident("top".to_string()));
        assert_eq!(tokens[2], Token::LParen);
        assert_eq!(tokens[3], Token::Input);
    }

    #[test]
    fn tokenize_gate_instance() {
        let input = "and g1(w, a, b);";
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0], Token::And);
        assert_eq!(tokens[1], Token::Ident("g1".to_string()));
    }

    #[test]
    fn tokenize_number_formats() {
        let input = "8'b10100101 4'hF 12 32'd100";
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0], Token::Number(0xA5));
        assert_eq!(tokens[1], Token::Number(0xF));
        assert_eq!(tokens[2], Token::Number(12));
        assert_eq!(tokens[3], Token::Number(100));
    }
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p rflux-verilog`

---

## Task 3: 实现 Parser

**Covers:** [S5]

**Files:**
- Create: `crates/verilog/src/parser.rs`

- [ ] **Step 1: 实现递归下降解析器**

```rust
use crate::ast::*;
use crate::lexer::{LexError, Lexer, Token};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
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
        // Parse optional port list in module header
        let mut ports = Vec::new();
        if self.peek() == &Token::LParen {
            self.advance();
            while self.peek() != &Token::RParen {
                ports.push(self.parse_port_decl()?);
                if self.peek() == &Token::Comma {
                    self.advance();
                }
            }
            self.expect(&Token::RParen)?;
        }
        self.expect(&Token::Semicolon)?;

        let mut items = Vec::new();
        while self.peek() != &Token::Endmodule && self.peek() != &Token::Eof {
            items.push(self.parse_module_item()?);
        }
        self.expect(&Token::Endmodule)?;

        Ok(VerilogModule { name, ports, items })
    }

    fn parse_port_decl(&mut self) -> Result<PortDecl, ParseError> {
        let direction = match self.peek() {
            Token::Input => { self.advance(); PortDirection::Input }
            Token::Output => { self.advance(); PortDirection::Output }
            Token::Inout => { self.advance(); PortDirection::Inout }
            other => return Err(self.error(format!("expected port direction, got {:?}", other))),
        };
        let range = self.parse_optional_range()?;
        let name = self.expect_ident()?;
        Ok(PortDecl { direction, name, range })
    }

    fn parse_module_item(&mut self) -> Result<ModuleItem, ParseError> {
        match self.peek().clone() {
            Token::Wire | Token::Reg => Ok(ModuleItem::Net(self.parse_net_decl()?)),
            Token::Assign => Ok(ModuleItem::Assign(self.parse_assign()?)),
            Token::Parameter => Ok(ModuleItem::Parameter(self.parse_parameter()?)),
            Token::Defparam => Ok(ModuleItem::Parameter(self.parse_defparam()?)),
            // Gate primitives
            Token::And | Token::Or | Token::Not | Token::Buf |
            Token::Xor | Token::Nand | Token::Nor | Token::Xnor |
            Token::Mux | Token::Dff => Ok(ModuleItem::Instance(self.parse_gate_instance()?)),
            // Module instantiation: ident ident ( ...
            Token::Ident(_) => Ok(ModuleItem::Instance(self.parse_module_instance()?)),
            other => Err(self.error(format!("unexpected token {:?} in module body", other))),
        }
    }

    fn parse_gate_instance(&mut self) -> Result<InstanceDecl, ParseError> {
        let module_name = match self.advance() {
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
            _ => unreachable!(),
        }.to_string();
        let name = self.expect_ident()?;
        self.expect(&Token::LParen)?;
        let connections = self.parse_port_connections()?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::Semicolon)?;
        Ok(InstanceDecl { module_name, name, connections })
    }

    fn parse_assign(&mut self) -> Result<Assignment, ParseError> {
        self.expect(&Token::Assign)?;
        let target = self.expect_ident()?;
        self.expect(&Token::Equals)?;
        let expr = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        Ok(Assignment { target, expr })
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_xor_expr()?;
        while self.peek() == &Token::Pipe {
            self.advance();
            let right = self.parse_xor_expr()?;
            left = Expr::BinOp(BinOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_xor_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and_expr()?;
        while self.peek() == &Token::Caret {
            self.advance();
            let right = self.parse_and_expr()?;
            left = Expr::BinOp(BinOp::Xor, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary_expr()?;
        while self.peek() == &Token::Amp {
            self.advance();
            let right = self.parse_unary_expr()?;
            left = Expr::BinOp(BinOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Tilde => {
                self.advance();
                let expr = self.parse_primary_expr()?;
                Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(expr)))
            }
            _ => self.parse_primary_expr(),
        }
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Ident(s) => { self.advance(); Ok(Expr::Ident(s)) }
            Token::Number(n) => { self.advance(); Ok(Expr::Literal(n)) }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            other => Err(self.error(format!("expected expression, got {:?}", other))),
        }
    }

    // Helper methods: peek(), advance(), expect(), expect_ident(), error()
}
```

需要实现完整的 helper 方法。

- [ ] **Step 2: 添加 parser 单元测试**

```rust
#[test]
fn parse_simple_module() {
    let input = r#"
        module top(input a, input b, output y);
            wire w;
            and g1(w, a, b);
            not g2(y, w);
        endmodule
    "#;
    let source = parse_verilog(input).unwrap();
    assert_eq!(source.modules.len(), 1);
    assert_eq!(source.modules[0].name, "top");
    assert_eq!(source.modules[0].ports.len(), 3);
    assert_eq!(source.modules[0].items.len(), 3);
}

#[test]
fn parse_assign_expression() {
    let input = r#"
        module top(input a, input b, output y);
            assign y = a & b;
        endmodule
    "#;
    let source = parse_verilog(input).unwrap();
    assert_eq!(source.modules[0].items.len(), 1);
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p rflux-verilog`

---

## Task 4: 实现 IR 转换 (Elaborator)

**Covers:** [S4]

**Files:**
- Create: `crates/verilog/src/elaborate.rs`

- [ ] **Step 1: 实现 elaborate_to_ir**

```rust
use crate::ast::*;
use rflux_ir::{LogicOp, Netlist, NodeKind};
use std::collections::HashMap;

pub fn elaborate_to_ir(source: &VerilogSource, top_module: &str) -> Result<Netlist, String> {
    let module = source.modules.iter()
        .find(|m| m.name == top_module)
        .or_else(|| source.modules.first())
        .ok_or("no module found")?;

    let mut netlist = Netlist::new();
    let mut wire_map: HashMap<String, rflux_ir::NodeId> = HashMap::new();
    let mut port_counter = 0u16;

    // 1. Create port nodes
    for port in &module.ports {
        let kind = NodeKind::Port;
        let node = netlist.add_node_with_logic(kind, port.name.clone(), None);
        wire_map.insert(port.name.clone(), node);
    }

    // 2. Process module items
    for item in &module.items {
        match item {
            ModuleItem::Instance(inst) => {
                let logic_op = gate_name_to_logic_op(&inst.module_name);
                let kind = if inst.module_name == "dff" {
                    NodeKind::Dff
                } else {
                    NodeKind::CellInstance
                };
                let node = netlist.add_node_with_logic(kind, inst.name.clone(), logic_op);

                // Connect ports
                // First connection is typically output, rest are inputs
                for (i, conn) in inst.connections.iter().enumerate() {
                    let signal_name = &conn.signal;
                    let target_node = *wire_map.entry(signal_name.clone()).or_insert_with(|| {
                        netlist.add_node(NodeKind::Port, signal_name.clone())
                    });
                    if i == 0 {
                        // Output: node drives the signal
                        netlist.connect(
                            rflux_ir::PinRef { node, port: 0 },
                            rflux_ir::PinRef { node: target_node, port: 0 },
                        ).ok();
                    } else {
                        // Input: signal drives the node
                        netlist.connect(
                            rflux_ir::PinRef { node: target_node, port: 0 },
                            rflux_ir::PinRef { node, port: i as u16 },
                        ).ok();
                    }
                }
            }
            ModuleItem::Assign(assign) => {
                // Lower assign to a gate
                let node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    format!("assign_{}", assign.target),
                    expr_to_logic_op(&assign.expr),
                );
                let target_node = *wire_map.entry(assign.target.clone()).or_insert_with(|| {
                    netlist.add_node(NodeKind::Port, assign.target.clone())
                });
                netlist.connect(
                    rflux_ir::PinRef { node, port: 0 },
                    rflux_ir::PinRef { node: target_node, port: 0 },
                ).ok();
                // Connect inputs from expression
                connect_expr_inputs(&mut netlist, &assign.expr, node, &mut wire_map, 1);
            }
            _ => {} // Wire/Reg declarations don't create nodes
        }
    }

    Ok(netlist)
}

fn gate_name_to_logic_op(name: &str) -> Option<LogicOp> {
    match name {
        "and" | "nand" => Some(LogicOp::And),
        "or" | "nor" => Some(LogicOp::Or),
        "not" | "buf" => Some(LogicOp::Not),
        "xor" | "xnor" => Some(LogicOp::Xor),
        "mux" => Some(LogicOp::Mux2),
        "dff" => Some(LogicOp::DffEnable),
        _ => None,
    }
}

fn expr_to_logic_op(expr: &Expr) -> Option<LogicOp> {
    match expr {
        Expr::BinOp(BinOp::And, _, _) => Some(LogicOp::And),
        Expr::BinOp(BinOp::Or, _, _) => Some(LogicOp::Or),
        Expr::BinOp(BinOp::Xor, _, _) => Some(LogicOp::Xor),
        Expr::UnaryOp(UnaryOp::Not, _) => Some(LogicOp::Not),
        _ => None,
    }
}
```

需要实现 `connect_expr_inputs` helper。

- [ ] **Step 2: 添加 elaboration 测试**

```rust
#[test]
fn elaborate_simple_and_gate() {
    let input = r#"
        module top(input a, input b, output y);
            and g1(y, a, b);
        endmodule
    "#;
    let source = parse_verilog(input).unwrap();
    let netlist = elaborate_to_ir(&source, "top").unwrap();
    assert!(netlist.node_count() >= 3); // a, b, y ports + and gate
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p rflux-verilog`

---

## Task 5: 集成到 io 和 CLI

**Covers:** [S6]

**Files:**
- Modify: `crates/io/Cargo.toml`
- Modify: `crates/io/src/lib.rs`
- Modify: `crates/cli/Cargo.toml`
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: io 添加 verilog 依赖**

在 `crates/io/Cargo.toml` 中添加：

```toml
rflux-verilog = { path = "../verilog" }
```

- [ ] **Step 2: 扩展 NetlistInputFormat**

在 `crates/io/src/lib.rs` 中，修改 `NetlistInputFormat`：

```rust
pub enum NetlistInputFormat {
    IrJson,
    Bench,
    Verilog,
}
```

- [ ] **Step 3: 添加 Verilog 解析路径**

在 `read_netlist` 和 `read_netlist_as` 函数中添加 Verilog 分支：

```rust
NetlistInputFormat::Verilog => {
    let content = fs::read_to_string(path)?;
    rflux_verilog::parse_verilog(&content)
        .and_then(|src| rflux_verilog::elaborate_to_ir(&src, "top"))
        .map_err(|e| IoError::BenchParse(e))
}
```

- [ ] **Step 4: CLI 格式检测**

在 CLI 的 `CliNetlistInputFormat` 枚举中添加 `Verilog` 变体，并在自动检测中根据 `.v` 扩展名选择。

- [ ] **Step 5: 运行测试**

Run: `cargo test -p rflux-io -p rflux-cli`

---

## Task 6: 最终验证

- [ ] **Step 1: 全量测试**

Run: `cargo test --workspace --exclude rflux-py`
Expected: 所有测试通过
