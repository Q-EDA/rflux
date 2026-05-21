use std::collections::{HashMap, HashSet};

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PinRef {
    pub node: NodeId,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeKind {
    CellInstance,
    MacroCell,
    Splitter,
    Dff,
    Jtl,
    Ptl,
    Port,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogicOp {
    Buf,
    And,
    Or,
    Xor,
    Mux2,
    DffEnable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logic_op: Option<LogicOp>,
}

#[derive(Debug, Error)]
pub enum IrError {
    #[error("source output pin is already connected; insert a splitter first")]
    SourceAlreadyConnected,
    #[error("destination input pin is already driven")]
    DestinationAlreadyDriven,
}

#[derive(Debug, Default, Clone)]
pub struct Netlist {
    nodes: Vec<Node>,
    edges: HashMap<PinRef, PinRef>,
    driven_inputs: HashSet<PinRef>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NetlistRepr {
    nodes: Vec<Node>,
    edges: Vec<(PinRef, PinRef)>,
}

impl Serialize for Netlist {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let repr = NetlistRepr {
            nodes: self.nodes.clone(),
            edges: self.edges.iter().map(|(from, to)| (*from, *to)).collect(),
        };
        repr.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Netlist {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let repr = NetlistRepr::deserialize(deserializer)?;

        let mut edges = HashMap::new();
        let mut driven_inputs = HashSet::new();
        for (from, to) in repr.edges {
            if edges.insert(from, to).is_some() {
                return Err(D::Error::custom("duplicate source pin in serialized netlist"));
            }
            if !driven_inputs.insert(to) {
                return Err(D::Error::custom("duplicate driven input pin in serialized netlist"));
            }
        }

        Ok(Self {
            nodes: repr.nodes,
            edges,
            driven_inputs,
        })
    }
}

impl Netlist {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, kind: NodeKind, name: impl Into<String>) -> NodeId {
        self.add_node_with_logic(kind, name, None)
    }

    pub fn add_node_with_logic(
        &mut self,
        kind: NodeKind,
        name: impl Into<String>,
        logic_op: Option<LogicOp>,
    ) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node {
            id,
            kind,
            name: name.into(),
            logic_op,
        });
        id
    }

    pub fn connect(&mut self, from: PinRef, to: PinRef) -> Result<(), IrError> {
        if self.edges.contains_key(&from) {
            return Err(IrError::SourceAlreadyConnected);
        }
        if self.driven_inputs.contains(&to) {
            return Err(IrError::DestinationAlreadyDriven);
        }

        self.edges.insert(from, to);
        self.driven_inputs.insert(to);
        Ok(())
    }

    pub fn sink_of(&self, from: PinRef) -> Option<PinRef> {
        self.edges.get(&from).copied()
    }

    pub fn disconnect(&mut self, from: PinRef) -> Option<PinRef> {
        let old_sink = self.edges.remove(&from)?;
        self.driven_inputs.remove(&old_sink);
        Some(old_sink)
    }

    pub fn is_input_driven(&self, to: PinRef) -> bool {
        self.driven_inputs.contains(&to)
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn edge_pairs(&self) -> Vec<(PinRef, PinRef)> {
        self.edges.iter().map(|(from, to)| (*from, *to)).collect()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_direct_fanout_without_splitter() {
        let mut netlist = Netlist::new();
        let src = netlist.add_node(NodeKind::CellInstance, "src");
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");

        let output = PinRef { node: src, port: 0 };
        let a_in = PinRef { node: a, port: 0 };
        let b_in = PinRef { node: b, port: 0 };

        netlist.connect(output, a_in).expect("first edge must pass");
        let err = netlist.connect(output, b_in).expect_err("fanout must fail");

        assert!(matches!(err, IrError::SourceAlreadyConnected));
    }
}
