use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerilogSource {
    pub modules: Vec<VerilogModule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerilogModule {
    pub name: String,
    pub ports: Vec<PortDecl>,
    pub items: Vec<ModuleItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDecl {
    pub direction: PortDirection,
    pub name: String,
    pub range: Option<(i32, i32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortDirection {
    Input,
    Output,
    Inout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModuleItem {
    Net(NetDecl),
    Instance(InstanceDecl),
    Assign(Assignment),
    Parameter(ParamDecl),
    AlwaysBlock(AlwaysBlock),
    GenerateBlock(GenerateBlock),
    TaskDecl(TaskDecl),
    FunctionDecl(FunctionDecl),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetDecl {
    pub kind: NetKind,
    pub name: String,
    pub range: Option<(i32, i32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetKind {
    Wire,
    Reg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceDecl {
    pub module_name: String,
    pub name: String,
    pub connections: Vec<PortConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortConnection {
    pub port_name: Option<String>,
    pub signal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    pub target: String,
    pub expr: Expr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDecl {
    pub name: String,
    pub value: i64,
}

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
    BlockingAssign {
        target: String,
        value: Expr,
    },
    NonBlockingAssign {
        target: String,
        value: Expr,
    },
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseItem {
    pub patterns: Vec<Expr>,
    pub body: Statement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    Ident(String),
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),
    Literal(i64),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Concat(Vec<Expr>),
    BitSelect(Box<Expr>, i32, i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    And,
    Or,
    Xor,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    LogicalAnd,
    LogicalOr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Not,
    Negate,
    LogicalNot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateBlock {
    pub label: Option<String>,
    pub kind: GenerateKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GenerateKind {
    For {
        init: GenVarInit,
        condition: Expr,
        step: GenVarStep,
        body: Vec<ModuleItem>,
    },
    If {
        condition: Expr,
        then_body: Vec<ModuleItem>,
        else_body: Option<Vec<ModuleItem>>,
    },
    Block(Vec<ModuleItem>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenVarInit {
    pub name: String,
    pub value: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenVarStep {
    pub name: String,
    pub op: GenVarOp,
    pub value: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenVarOp {
    AddAssign,
    SubAssign,
    Assign,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDecl {
    pub name: String,
    pub ports: Vec<TaskPort>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPort {
    pub direction: PortDirection,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDecl {
    pub name: String,
    pub return_range: Option<(i32, i32)>,
    pub ports: Vec<TaskPort>,
    pub body: Vec<Statement>,
}
