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
pub enum Expr {
    Ident(String),
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),
    Literal(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    And,
    Or,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Not,
}
