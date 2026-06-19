# Verilog Phase 2：行为级 RTL 支持

日期：2026-06-19

## [S1] 概述

扩展 rflux-verilog crate 支持行为级 Verilog，包括 always 块、if/case 语句、运算符、阻塞/非阻塞赋值。将行为级描述转换为 SFQ 门级 IR。

## [S2] AST 扩展

### Always 块

```rust
pub struct AlwaysBlock {
    pub sensitivity: SensitivityList,
    pub body: Statement,
}

pub struct SensitivityList {
    pub items: Vec<SensitivityItem>,
}

pub enum SensitivityItem {
    Posedge(String),
    Negedge(String),
    All,
}
```

### 语句

```rust
pub enum Statement {
    Block(Vec<Statement>),
    If { condition: Expr, then_body: Box<Statement>, else_body: Option<Box<Statement>> },
    Case { expr: Expr, items: Vec<CaseItem> },
    BlockingAssign { target: String, value: Expr },
    NonBlockingAssign { target: String, value: Expr },
}

pub struct CaseItem {
    pub patterns: Vec<Expr>,
    pub body: Statement,
}
```

### 运算符扩展

```rust
pub enum BinOp {
    And, Or, Xor,
    Add, Sub, Mul, Div, Mod,
    Eq, Neq, Lt, Gt, Le, Ge,
    Shl, Shr,
    BitAnd, BitOr, BitXor,
}

pub enum UnaryOp {
    Not,
    Negate,
}
```

### 表达式扩展

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

## [S3] Lexer 扩展

新增关键字：Always, If, Else, Case, Casex, Casez, Endcase, Begin, End, Posedge, Negedge

新增符号：`==`, `!=`, `<=`, `>=`, `<<`, `>>`, `?`, `+`, `-`, `*`, `/`, `%`, `!`, `&&`, `||`

## [S4] Parser 扩展

- `parse_always_block()`：解析 sensitivity list + body
- `parse_statement()`：分发到 if/case/block/assign
- `parse_if_statement()`：if/else if/else
- `parse_case_statement()`：case/endcase
- 表达式解析器：扩展优先级（算术 > 比较 > 位运算 > 逻辑）

## [S5] Elaborator 扩展

### 组合逻辑（always @(*)）

- `if (cond) y = a; else y = b;` → MUX2(cond, a, b)
- `case (sel) 0: y = a; 1: y = b;` → MUX2(sel, a, b)
- `y = a & b;` → AND 门
- `y = a | b;` → OR 门
- `y = ~a;` → NOT 门
- `y = a ^ b;` → XOR 门
- `y = a + b;` → 降低为门级加法器（ripple carry）
- `y = a == b;` → XNOR
- `y = a ? b : c;` → MUX2(a, b, c)

### 时序逻辑（always @(posedge clk)）

- `q <= d;` → DFF(clk, d, q)
- `if (en) q <= d;` → DFF with enable

## [S6] 测试策略

- Lexer：新关键字和符号的 token 化
- Parser：always 块、if/case、运算符表达式的 AST 正确性
- Elaborator：行为级 → 门级 IR 转换正确性
- 端到端：`.v` 文件 → parse → elaborate → compile-netlist
