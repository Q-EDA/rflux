# Verilog 前端 Phase 1：结构化 Verilog 解析

日期：2026-06-19

## [S1] 概述

为 rflux 添加结构化 Verilog 网表解析能力，支持门级 Verilog 作为输入格式。Phase 1 覆盖 module 声明、端口、wire/reg、门级实例化和 assign 连续赋值。

## [S2] 支持的 Verilog 语法

### Module 声明
```verilog
module top(input a, input b, output y);
  wire w1;
  and g1(w1, a, b);
  not g2(y, w1);
endmodule
```

### 端口声明
- `input [msb:lsb] name` / `input name`
- `output [msb:lsb] name` / `output name`
- `inout [msb:lsb] name`

### 网线/寄存器声明
- `wire name` / `wire [msb:lsb] name`
- `reg name` / `reg [msb:lsb] name`

### 门级实例化
- `and g1(out, in1, in2)`
- `or g2(out, in1, in2, in3)`
- `not g3(out, in)`
- `buf g4(out, in)`
- `xor g5(out, in1, in2)`
- `nand`, `nor`, `xnor`（降低为 And/Or/Xor + Not）
- `mux21 g6(out, sel, in0, in1)`
- `dff g7(clk, d, q)`

### 连续赋值
- `assign w = a & b;`（降低为 And 门）
- `assign w = ~a;`（降低为 Not 门）
- 支持 &, |, ^, ~ 运算符

### 参数
- `parameter NAME = value;`
- `defparam inst.param = value;`

## [S3] AST 结构

```rust
pub struct VerilogSource {
    pub modules: Vec<VerilogModule>,
}

pub struct VerilogModule {
    pub name: String,
    pub ports: Vec<PortDecl>,
    pub items: Vec<ModuleItem>,
}

pub struct PortDecl {
    pub direction: PortDirection,
    pub name: String,
    pub range: Option<(i32, i32)>,
}

pub enum PortDirection { Input, Output, Inout }

pub enum ModuleItem {
    Net(NetDecl),
    Instance(InstanceDecl),
    Assign(Assignment),
    Parameter(ParamDecl),
}

pub struct NetDecl {
    pub kind: NetKind,
    pub name: String,
    pub range: Option<(i32, i32)>,
}

pub enum NetKind { Wire, Reg }

pub struct InstanceDecl {
    pub module_name: String,
    pub name: String,
    pub connections: Vec<PortConnection>,
}

pub struct PortConnection {
    pub port_name: Option<String>,
    pub signal: String,
}

pub struct Assignment {
    pub target: String,
    pub expr: Expr,
}

pub struct ParamDecl {
    pub name: String,
    pub value: i64,
}

pub enum Expr {
    Ident(String),
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),
    Literal(i64),
}

pub enum BinOp { And, Or, Xor }
pub enum UnaryOp { Not }
```

## [S4] IR 转换

从 VerilogSource 转换为 rflux Netlist：
1. 创建顶层模块对应的 Netlist
2. 端口 → Port 节点
3. 门实例化 → CellInstance 节点 + LogicOp
4. assign 运算 → 合成的逻辑门节点
5. wire 声明 → 连接关系

## [S5] Lexer/Parser 实现

使用手写递归下降解析器（避免外部依赖）：
- Lexer：逐字符扫描，生成 Token 流
- Parser：递归下降，构建 AST
- 错误报告：行号 + 列号 + 描述

## [S6] 集成点

1. `crates/verilog/`：新增 crate，包含 lexer/parser/ast/elaborator
2. `crates/io/`：添加 `NetlistInputFormat::Verilog` 变体
3. `crates/cli/`：`--input-format verilog` 支持
4. Python：`read_verilog()` 绑定

## [S7] 测试策略

- Lexer 单元测试：token 类型正确性
- Parser 单元测试：AST 结构正确性
- 集成测试：解析 → IR 转换 → compile-netlist 完整流程
- 参考 .bench 测试用例的 Verilog 版本
