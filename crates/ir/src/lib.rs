use std::collections::{HashMap, HashSet};

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Unique identifier for a node within a [Netlist].
///
/// Internally wraps a usize index. Created by [`Netlist::add_node`]
/// and [`Netlist::add_node_with_logic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub usize);

/// Reference to a specific pin on a node.
///
/// Combines a [`NodeId`] with a port number so callers can
/// distinguish multiple input/output pins on the same cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PinRef {
    pub node: NodeId,
    pub port: u16,
}

/// High-level category of a node in the netlist.
///
/// Each variant corresponds to an SFQ circuit primitive or a
/// user-facing port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeKind {
    /// A standard logic cell (gate).
    CellInstance,
    /// A pre-characterised multi-level macro (no internal clock
    /// input exposed to the router).
    MacroCell,
    /// Explicit fan-out splitter.
    Splitter,
    /// Delay flip-flop (preserves pulse ordering).
    Dff,
    /// Josephson transmission line segment (active interconnect).
    Jtl,
    /// Passive transmission line segment.
    Ptl,
    /// Top-level input or output port.
    Port,
}

/// Boolean / sequential operation performed by a cell.
///
/// Only meaningful when [`NodeKind::CellInstance`]; other node
/// kinds always carry None.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogicOp {
    Buf,
    Not,
    And,
    Or,
    Xor,
    Mux2,
    DffEnable,
}

/// A single node in the netlist graph.
///
/// Every node has a stable [`NodeId`], a [`NodeKind`], a
/// human-readable name, and an optional [`LogicOp`] that
/// describes its behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logic_op: Option<LogicOp>,
}

/// Errors that can arise when mutating a [Netlist].
#[derive(Debug, Error)]
pub enum IrError {
    /// The source output pin is already driving a destination.
    /// SFQ requires a [Splitter](NodeKind::Splitter) for fan-out.
    #[error("source output pin is already connected; insert a splitter first")]
    SourceAlreadyConnected,
    /// The destination input pin is already driven by a source.
    #[error("destination input pin is already driven")]
    DestinationAlreadyDriven,
}

impl IrError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            IrError::SourceAlreadyConnected => "RFLOW-FLOW-001",
            IrError::DestinationAlreadyDriven => "RFLOW-FLOW-001",
        }
    }

    #[must_use]
    pub fn suggestion(&self) -> &'static str {
        match self {
            IrError::SourceAlreadyConnected => {
                "Insert a Splitter node before the destination to enable fan-out."
            }
            IrError::DestinationAlreadyDriven => {
                "Each input pin may only be driven by one source. Check for duplicate connections."
            }
        }
    }
}

/// A directed graph representing an SFQ netlist.
///
/// Each node has one or more **pins** (input / output ports).
/// An edge connects exactly one source pin to one destination
/// pin, enforcing single-driver semantics:
///
/// - **Single consumer:** a source pin can drive at most one
///   destination (fan-out requires an explicit [Splitter](NodeKind::Splitter)).
/// - **Single driver:** a destination pin can be driven by at
///   most one source.
///
/// Serialises to and from JSON via serde.
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
                return Err(D::Error::custom(
                    "duplicate source pin in serialized netlist",
                ));
            }
            if !driven_inputs.insert(to) {
                return Err(D::Error::custom(
                    "duplicate driven input pin in serialized netlist",
                ));
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
    /// Creates an empty netlist.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a node with the given kind and name, returning its stable [`NodeId`].
    pub fn add_node(&mut self, kind: NodeKind, name: impl Into<String>) -> NodeId {
        self.add_node_with_logic(kind, name, None)
    }

    /// Appends a node with an optional [`LogicOp`].
    ///
    /// Use this when the cell's behaviour matters for synthesis
    /// or verification. The [`NodeKind`] alone is insufficient
    /// to distinguish, e.g., an AND gate from an OR gate.
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

    /// Connects a source pin to a destination pin.
    ///
    /// Returns [`IrError::SourceAlreadyConnected`] if the source
    /// pin already drives another pin, and
    /// [`IrError::DestinationAlreadyDriven`] if the destination
    /// is already driven by another source.
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

    /// Returns the destination of the edge starting at rom, if any.
    #[must_use]
    pub fn sink_of(&self, from: PinRef) -> Option<PinRef> {
        self.edges.get(&from).copied()
    }

    /// Removes the edge starting at rom, returning the removed
    /// destination (if any).
    pub fn disconnect(&mut self, from: PinRef) -> Option<PinRef> {
        let old_sink = self.edges.remove(&from)?;
        self.driven_inputs.remove(&old_sink);
        Some(old_sink)
    }

    /// Returns true if a given input pin is already driven.
    #[must_use]
    pub fn is_input_driven(&self, to: PinRef) -> bool {
        self.driven_inputs.contains(&to)
    }

    /// All nodes in the netlist, indexed by [`NodeId`].
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Mutable reference to a single node, or None if the id is out of range.
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.0)
    }

    /// All edges as a flat vector of (source, destination) pairs.
    #[must_use]
    pub fn edge_pairs(&self) -> Vec<(PinRef, PinRef)> {
        self.edges.iter().map(|(from, to)| (*from, *to)).collect()
    }

    /// Number of nodes in the netlist.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of edges in the netlist.
    #[must_use]
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

    #[test]
    fn ir_error_codes_are_stable() {
        assert_eq!(IrError::SourceAlreadyConnected.code(), "RFLOW-FLOW-001");
        assert_eq!(
            IrError::DestinationAlreadyDriven.code(),
            "RFLOW-FLOW-001"
        );
        assert!(!IrError::SourceAlreadyConnected.suggestion().is_empty());
        assert!(!IrError::DestinationAlreadyDriven.suggestion().is_empty());
    }
}
