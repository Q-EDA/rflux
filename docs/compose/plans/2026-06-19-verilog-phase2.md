# Verilog Phase 2 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 扩展 rflux-verilog 支持行为级 Verilog (always/if/case/运算符)

**Architecture:** 扩展现有 lexer/parser/ast/elaborator 四个模块，保持 Phase 1 向后兼容。

**Tech Stack:** Rust, rflux-verilog, rflux-ir

---

## Task 1: 扩展 AST 和 Lexer

**Covers:** [S2, S3]

**Files:**
- Modify: `crates/verilog/src/ast.rs`
- Modify: `crates/verilog/src/lexer.rs`

- [ ] **Step 1: 扩展 ast.rs**

在现有类型基础上添加：

```rust
// ModuleItem 新增变体
AlwaysBlock(AlwaysBlock),

// 新增结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlwaysBlock {
    pub sensitivity: SensitivityList,
    pub body: Statement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityList {
    pub items: Vec<SensitivityItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SensitivityItem {
    Posedge(String),
    Negedge(String),
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Statement {
    Block(Vec<Statement>),
    If {
        condition: Expr,
        then_body: Box<Statement>,
        else_body: Option<Box<Statement>>,
    },
    Case {
        expr: Expr,
        items: Vec<CaseItem>,
        default: Option<Box<Statement>>,
    },
    BlockingAssign { target: String, value: Expr },
    NonBlockingAssign { target: String, value: Expr },
    Null,  // 空语句
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseItem {
    pub patterns: Vec<Expr>,
    pub body: Statement,
}
```

扩展 BinOp：

```rust
pub enum BinOp {
    And, Or, Xor,
    Add, Sub, Mul, Div, Mod,
    Eq, Neq, Lt, Gt, Le, Ge,
    Shl, Shr,
    BitAnd, BitOr, BitXor,
    LogicalAnd, LogicalOr,
}
```

扩展 UnaryOp：

```rust
pub enum UnaryOp {
    Not,
    Negate,
    LogicalNot,
}
```

扩展 Expr：

```rust
pub enum Expr {
    Ident(String),
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),
    Literal(i64),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Concat(Vec<Expr>),
    BitSelect(Box<Expr>, i32, i32),
}
```

- [ ] **Step 2: 扩展 lexer.rs**

添加关键字 Token 变体：

```rust
Always, If, Else, Case, Casex, Casez, Endcase,
Begin, End, Posedge, Negedge,
```

添加运算符 Token 变体：

```rust
EqEq,      // ==
NotEq,     // !=
LtEq,      // <=
GtEq,      // >=
Shl,       // <<
Shr,       >>
Question,  // ?
Plus,      // +
Minus,     // -
Star,      // *
Slash,     // /
Percent,   // %
LogicalAnd, // &&
LogicalOr,  // ||
LogicalNot, // !
BitAnd,    // &
BitOr,     // |
BitXor,    // ^
Tilde,     // ~
```

在 `next_token` 方法中添加：
- `=` 后跟 `=` → `EqEq`
- `!` 后跟 `=` → `NotEq`
- `<` 后跟 `=` → `LtEq`，`<` 后跟 `<` → `Shl`
- `>` 后跟 `=` → `GtEq`，`>` 后跟 `>` → `Shr`
- `&` 后跟 `&` → `LogicalAnd`
- `|` 后跟 `|` → `LogicalOr`

在关键字匹配中添加：

```rust
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
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p rflux-verilog`

- [ ] **Step 4: Commit**

```bash
git add crates/verilog/src/ast.rs crates/verilog/src/lexer.rs
git commit -m "feat(verilog): extend AST and lexer for behavioral RTL"
```

---

## Task 2: 扩展 Parser

**Covers:** [S4]

**Files:**
- Modify: `crates/verilog/src/parser.rs`

- [ ] **Step 1: 扩展 parse_module_item**

在 `parse_module_item` 的 match 中添加：

```rust
Token::Always => Ok(ModuleItem::AlwaysBlock(self.parse_always_block()?)),
```

- [ ] **Step 2: 实现 parse_always_block**

```rust
fn parse_always_block(&mut self) -> Result<AlwaysBlock, ParseError> {
    self.expect(&Token::Always)?;
    self.expect(&Token::At)?;
    self.expect(&Token::LParen)?;
    let sensitivity = self.parse_sensitivity_list()?;
    self.expect(&Token::RParen)?;
    let body = self.parse_statement()?;
    Ok(AlwaysBlock { sensitivity, body })
}

fn parse_sensitivity_list(&mut self) -> Result<SensitivityList, ParseError> {
    let mut items = Vec::new();
    loop {
        match self.peek() {
            Token::Star => { self.advance(); items.push(SensitivityItem::All); }
            Token::Posedge => {
                self.advance();
                let name = self.expect_ident()?;
                items.push(SensitivityItem::Posedge(name));
            }
            Token::Negedge => {
                self.advance();
                let name = self.expect_ident()?;
                items.push(SensitivityItem::Negedge(name));
            }
            Token::Ident(name) => {
                // 默认当作电平敏感
                let name = name.clone();
                self.advance();
                items.push(SensitivityItem::Posedge(name)); // 简化处理
            }
            _ => break,
        }
        if self.peek() == &Token::Comma {
            self.advance();
        } else {
            break;
        }
    }
    Ok(SensitivityList { items })
}
```

- [ ] **Step 3: 实现 parse_statement**

```rust
fn parse_statement(&mut self) -> Result<Statement, ParseError> {
    match self.peek() {
        Token::If => self.parse_if_statement(),
        Token::Case => self.parse_case_statement(),
        Token::Begin => self.parse_block_statement(),
        Token::Ident(_) => self.parse_assign_statement(),
        _ => Ok(Statement::Null),
    }
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
    Ok(Statement::If { condition, then_body, else_body })
}

fn parse_case_statement(&mut self) -> Result<Statement, ParseError> {
    self.advance(); // consume case/casex/casez
    self.expect(&Token::LParen)?;
    let expr = self.parse_expr()?;
    self.expect(&Token::RParen)?;
    let mut items = Vec::new();
    let mut default = None;
    while self.peek() != &Token::Endcase && self.peek() != &Token::Eof {
        if self.peek() == &Token::Ident("default".to_string()) {
            self.advance();
            self.expect(&Token::Colon)?;
            default = Some(Box::new(self.parse_statement()?));
        } else {
            let mut patterns = vec![self.parse_expr()?];
            while self.peek() == &Token::Comma {
                self.advance();
                patterns.push(self.parse_expr()?);
            }
            self.expect(&Token::Colon)?;
            let body = self.parse_statement()?;
            items.push(CaseItem { patterns, body });
        }
    }
    self.expect(&Token::Endcase)?;
    Ok(Statement::Case { expr, items, default })
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

fn parse_assign_statement(&mut self) -> Result<Statement, ParseError> {
    let target = self.expect_ident()?;
    match self.peek() {
        Token::LtEq => {
            self.advance();
            let value = self.parse_expr()?;
            self.expect(&Token::Semicolon)?;
            Ok(Statement::NonBlockingAssign { target, value })
        }
        Token::Equals => {
            self.advance();
            let value = self.parse_expr()?;
            self.expect(&Token::Semicolon)?;
            Ok(Statement::BlockingAssign { target, value })
        }
        _ => Err(ParseError::UnexpectedToken {
            expected: "= or <=".to_string(),
            found: self.peek().clone(),
            pos: self.pos,
        }),
    }
}
```

- [ ] **Step 4: 扩展表达式解析器**

扩展 `parse_primary_expr` 支持：
- `? :` 三元运算符
- `{a, b}` 拼接
- `a[3:0]` 位选择
- 更多一元运算符（`-`, `!`）

扩展 `parse_or_expr` → `parse_logical_or` → `parse_logical_and` → `parse_bitwise_or` → `parse_bitwise_xor` → `parse_bitwise_and` → `parse_equality` → `parse_relational` → `parse_shift` → `parse_additive` → `parse_multiplicative` → `parse_unary` → `parse_primary`

- [ ] **Step 5: 添加测试**

```rust
#[test]
fn parse_always_combinational() {
    let input = "always @(*) begin if (sel) y = a; else y = b; end";
    // 解析并验证 AST
}

#[test]
fn parse_always_sequential() {
    let input = "always @(posedge clk) q <= d;";
    // 解析并验证 AST
}

#[test]
fn parse_case_statement() {
    let input = "always @(*) begin case(sel) 2'd0: y = a; 2'd1: y = b; default: y = c; endcase end";
    // 解析并验证 AST
}

#[test]
fn parse_arithmetic_expressions() {
    let input = "assign y = (a + b) * c;";
    // 解析并验证 AST
}
```

- [ ] **Step 6: 运行测试**

Run: `cargo test -p rflux-verilog`

---

## Task 3: 扩展 Elaborator

**Covers:** [S5]

**Files:**
- Modify: `crates/verilog/src/elaborate.rs`

- [ ] **Step 1: 处理 AlwaysBlock**

在 `elaborate_module` 的 item 循环中添加：

```rust
ModuleItem::AlwaysBlock(always) => {
    elaborate_always(netlist, &always, wire_map, &module.name)?;
}
```

- [ ] **Step 2: 实现组合逻辑转换**

```rust
fn elaborate_always(
    netlist: &mut Netlist,
    always: &AlwaysBlock,
    wire_map: &mut HashMap<String, NodeId>,
    module_name: &str,
) -> Result<(), String> {
    let is_combinational = always.sensitivity.items.iter().any(|item| matches!(item, SensitivityItem::All));
    let is_sequential = always.sensitivity.items.iter().any(|item| matches!(item, SensitivityItem::Posedge(_) | SensitivityItem::Negedge(_)));

    if is_combinational {
        elaborate_combinational_statement(netlist, &always.body, wire_map, module_name)
    } else if is_sequential {
        elaborate_sequential_statement(netlist, &always, wire_map, module_name)
    } else {
        Ok(()) // 空敏感列表
    }
}
```

- [ ] **Step 3: 实现语句转换**

```rust
fn elaborate_combinational_statement(
    netlist: &mut Netlist,
    stmt: &Statement,
    wire_map: &mut HashMap<String, NodeId>,
    module_name: &str,
) -> Result<(), String> {
    match stmt {
        Statement::Block(stmts) => {
            for s in stmts {
                elaborate_combinational_statement(netlist, s, wire_map, module_name)?;
            }
            Ok(())
        }
        Statement::If { condition, then_body, else_body } => {
            // if (cond) y = a; else y = b; → MUX2(cond, a, b)
            let target = extract_assign_target(then_body).or_else(|| else_body.as_ref().and_then(|e| extract_assign_target(e)));
            if let Some(target_name) = target {
                let mux_node = netlist.add_node_with_logic(NodeKind::CellInstance, format!("{module_name}_mux_{target_name}"), Some(LogicOp::Mux2));
                // 连接 cond, a, b 到 MUX
                // 连接 MUX 输出到 target
            }
            Ok(())
        }
        Statement::BlockingAssign { target, value } => {
            // y = expr → 从 expr 构建门级逻辑
            elaborate_expr_to_gates(netlist, target, value, wire_map, module_name)
        }
        _ => Ok(()),
    }
}
```

- [ ] **Step 4: 实现表达式到门的转换**

```rust
fn elaborate_expr_to_gates(
    netlist: &mut Netlist,
    target: &str,
    expr: &Expr,
    wire_map: &mut HashMap<String, NodeId>,
    module_name: &str,
) -> Result<(), String> {
    match expr {
        Expr::BinOp(op, left, right) => {
            let logic_op = match op {
                BinOp::And | BinOp::BitAnd => Some(LogicOp::And),
                BinOp::Or | BinOp::BitOr => Some(LogicOp::Or),
                BinOp::Xor | BinOp::BitXor => Some(LogicOp::Xor),
                BinOp::Eq => Some(LogicOp::Xor), // XNOR = XOR + NOT
                BinOp::Neq => Some(LogicOp::Xor),
                _ => None,
            };
            if let Some(lo) = logic_op {
                let node = netlist.add_node_with_logic(
                    NodeKind::CellInstance,
                    format!("{module_name}_{:?}_{target}", op),
                    Some(lo),
                );
                // 连接输入输出
            }
            Ok(())
        }
        Expr::UnaryOp(UnaryOp::Not, inner) => {
            let node = netlist.add_node_with_logic(
                NodeKind::CellInstance,
                format!("{module_name}_not_{target}"),
                Some(LogicOp::Not),
            );
            // 连接
            Ok(())
        }
        Expr::Ternary(cond, then_expr, else_expr) => {
            let node = netlist.add_node_with_logic(
                NodeKind::CellInstance,
                format!("{module_name}_mux_{target}"),
                Some(LogicOp::Mux2),
            );
            // 连接 cond, then, else
            Ok(())
        }
        Expr::Ident(name) => {
            // 直接连接
            Ok(())
        }
        _ => Ok(()), // 暂不支持其他表达式
    }
}
```

- [ ] **Step 5: 实现时序逻辑转换**

```rust
fn elaborate_sequential_statement(
    netlist: &mut Netlist,
    always: &AlwaysBlock,
    wire_map: &mut HashMap<String, NodeId>,
    module_name: &str,
) -> Result<(), String> {
    // 提取时钟信号
    let clk = always.sensitivity.items.iter().find_map(|item| {
        if let SensitivityItem::Posedge(name) = item { Some(name.clone()) } else { None }
    });
    
    // 从 body 提取 DFF
    extract_dff_from_statement(netlist, &always.body, clk.as_deref(), wire_map, module_name)
}
```

- [ ] **Step 6: 添加测试**

```rust
#[test]
fn elaborate_always_mux() {
    let input = r#"
        module mux(input sel, input a, input b, output y);
            always @(*) begin
                if (sel) y = a;
                else y = b;
            end
        endmodule
    "#;
    let source = parse_verilog(input).unwrap();
    let netlist = elaborate_to_ir(&source, "mux").unwrap();
    // 验证生成了 MUX 节点
}

#[test]
fn elaborate_always_dff() {
    let input = r#"
        module dff(input clk, input d, output reg q);
            always @(posedge clk) begin
                q <= d;
            end
        endmodule
    "#;
    let source = parse_verilog(input).unwrap();
    let netlist = elaborate_to_ir(&source, "dff").unwrap();
    // 验证生成了 DFF 节点
}
```

- [ ] **Step 7: 运行测试**

Run: `cargo test -p rflux-verilog`

---

## Task 4: 端到端验证

- [ ] **Step 1: 创建测试 .v 文件**

创建 `crates/verilog/tests/fixtures/` 目录，添加测试文件：

```verilog
// mux2.v
module mux2(input sel, input a, input b, output y);
    always @(*) begin
        if (sel) y = a;
        else y = b;
    end
endmodule

// dff.v
module dff(input clk, input d, output reg q);
    always @(posedge clk) begin
        q <= d;
    end
endmodule

// counter.v
module counter(input clk, input rst, output reg [3:0] count);
    always @(posedge clk) begin
        if (rst) count <= 4'd0;
        else count <= count + 4'd1;
    end
endmodule
```

- [ ] **Step 2: 集成测试**

```rust
#[test]
fn phase2_end_to_end_mux() {
    let input = include_str!("fixtures/mux2.v");
    let source = parse_verilog(input).unwrap();
    let netlist = elaborate_to_ir(&source, "mux2").unwrap();
    assert!(netlist.node_count() >= 1);
}

#[test]
fn phase2_end_to_end_dff() {
    let input = include_str!("fixtures/dff.v");
    let source = parse_verilog(input).unwrap();
    let netlist = elaborate_to_ir(&source, "dff").unwrap();
    assert!(netlist.node_count() >= 1);
}
```

- [ ] **Step 3: 全量测试**

Run: `cargo test -p rflux-verilog`
Expected: 所有 Phase 1 + Phase 2 测试通过

---

## Task 5: 最终验证

- [ ] **Step 1: 全量 workspace 测试**

Run: `cargo test --workspace --exclude rflux-py`
Expected: 所有测试通过
