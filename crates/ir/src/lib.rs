use std::collections::{HashMap, HashSet};
use std::fmt;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Pulse-level IR types (P0-1: dataflow semantics)
// ---------------------------------------------------------------------------

/// Identifier for a clock domain within a [`Netlist`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClockDomainId(pub usize);

/// A clock domain groups nodes that share a common clock signal.
///
/// In SFQ every gate receives its own clock pulse; a domain captures
/// the logical grouping (same frequency, fixed phase relationship)
/// rather than a single physical wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockDomain {
    pub id: ClockDomainId,
    pub name: String,
    /// Clock frequency in GHz.
    pub frequency_ghz: f64,
    /// Phase offset from the reference domain, in radians [0, 2π).
    #[serde(default)]
    pub phase_rad: f64,
    /// Number of clock phases in this domain (1 = single-phase,
    /// 2 = dual-phase, etc.).
    #[serde(default = "default_phases")]
    pub phases: u32,
}

fn default_phases() -> u32 {
    1
}

/// Time window in picoseconds during which a pulse is valid on an edge.
///
/// SFQ timing verification checks that the pulse arrival window
/// overlaps the receiving gate's capture window.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PulseWindow {
    /// Earliest possible pulse arrival time (ps).
    pub earliest_ps: f64,
    /// Latest possible pulse arrival time (ps).
    pub latest_ps: f64,
}

impl PulseWindow {
    /// Creates a new window. Returns `None` if `latest < earliest`.
    #[must_use]
    pub fn new(earliest_ps: f64, latest_ps: f64) -> Option<Self> {
        if latest_ps < earliest_ps {
            None
        } else {
            Some(Self {
                earliest_ps,
                latest_ps,
            })
        }
    }

    /// Duration of the window in ps.
    #[must_use]
    pub fn width_ps(&self) -> f64 {
        self.latest_ps - self.earliest_ps
    }

    /// Returns `true` if `other` overlaps with this window.
    #[must_use]
    pub fn overlaps(&self, other: &PulseWindow) -> bool {
        self.earliest_ps <= other.latest_ps && other.earliest_ps <= self.latest_ps
    }

    /// Shifts the entire window by `delta_ps`.
    pub fn shift(&mut self, delta_ps: f64) {
        self.earliest_ps += delta_ps;
        self.latest_ps += delta_ps;
    }
}

impl fmt::Display for PulseWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:.2}, {:.2}] ps", self.earliest_ps, self.latest_ps)
    }
}

/// Metadata attached to a netlist edge describing its pulse-level
/// properties.  Stored in [`Netlist::pulse_edges`], keyed by the
/// source [`PinRef`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PulseEdge {
    /// Pulse arrival window at the sink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pulse_window: Option<PulseWindow>,
    /// Propagation delay in picoseconds (JTL/PTL + wire).
    #[serde(default)]
    pub delay_ps: f64,
    /// Whether this edge is part of a path-balance–critical path.
    #[serde(default)]
    pub balance_critical: bool,
    /// Clock domain of the source node, if known at IR level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_domain: Option<ClockDomainId>,
}

impl Default for PulseEdge {
    fn default() -> Self {
        Self {
            pulse_window: None,
            delay_ps: 0.0,
            balance_critical: false,
            clock_domain: None,
        }
    }
}

/// Describes a constraint that two paths to the same DFF (or output)
/// must have matching delays within a tolerance.
///
/// SFQ's destructive readout means a pulse consumed on one path
/// cannot be re-read; if the other path arrives at a different
/// clock phase the circuit malfunctions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathBalanceConstraint {
    /// Human-readable label (e.g. "dff3_data_a vs dff3_data_b").
    pub name: String,
    /// Source pin of path A.
    pub path_a_start: PinRef,
    /// Source pin of path B.
    pub path_b_start: PinRef,
    /// Destination pin where both paths must arrive at the same
    /// clock phase.
    pub convergence_point: PinRef,
    /// Maximum allowed delay difference in ps.  Typically one clock
    /// period minus the DFF setup time.
    pub tolerance_ps: f64,
    /// Associated clock domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_domain: Option<ClockDomainId>,
}

/// Identifies which pins on a [`NodeKind::MacroCell`] are clock
/// boundary ports versus internal data ports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroCellBoundary {
    /// Pins that receive a clock signal at the macro boundary.
    pub clock_pins: Vec<PinRef>,
    /// Pins that carry data into/out of the macro.
    pub data_pins: Vec<PinRef>,
}

/// Unique identifier for a node within a [Netlist].
///
/// Internally wraps a usize index. Created by [`Netlist::add_node`]
/// and [`Netlist::add_node_with_logic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub usize);

/// Reference to a specific pin on a node.
///
/// Combines a [`NodeId`] with a port number so callers can
/// distinguish multiple input/output pins on the same cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
///
/// Pulse-level annotations are optional and degrade gracefully:
/// code that only needs the netlist topology can ignore them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logic_op: Option<LogicOp>,
    /// Clock domain this node belongs to (pulse-level IR).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_domain: Option<ClockDomainId>,
    /// Macro-cell boundary annotation (only meaningful for
    /// [`NodeKind::MacroCell`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macro_boundary: Option<MacroCellBoundary>,
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
/// The netlist also carries optional **pulse-level metadata**:
/// - [`PulseEdge`] per edge (arrival windows, delay, balance flag)
/// - [`ClockDomain`] definitions
/// - [`PathBalanceConstraint`] for destructive-readout path matching
///
/// All pulse-level fields are `Default`-constructed when absent,
/// so existing code that only touches `nodes` / `edges` continues
/// to work without modification.
///
/// Serialises to and from JSON via serde.
#[derive(Debug, Default, Clone)]
pub struct Netlist {
    nodes: Vec<Node>,
    edges: HashMap<PinRef, PinRef>,
    driven_inputs: HashSet<PinRef>,
    // --- pulse-level extensions (P0-1) ---
    /// Per-edge pulse metadata, keyed by source [`PinRef`].
    pulse_edges: HashMap<PinRef, PulseEdge>,
    /// Clock domain definitions.
    clock_domains: Vec<ClockDomain>,
    /// Path balance constraints.
    path_balance_constraints: Vec<PathBalanceConstraint>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NetlistRepr {
    nodes: Vec<Node>,
    edges: Vec<(PinRef, PinRef)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pulse_edges: Vec<(PinRef, PinRef, PulseEdge)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    clock_domains: Vec<ClockDomain>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    path_balance_constraints: Vec<PathBalanceConstraint>,
}

impl Serialize for Netlist {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let repr = NetlistRepr {
            nodes: self.nodes.clone(),
            edges: self.edges.iter().map(|(from, to)| (*from, *to)).collect(),
            pulse_edges: self
                .pulse_edges
                .iter()
                .map(|(from, meta)| (*from, self.edges[from], meta.clone()))
                .collect(),
            clock_domains: self.clock_domains.clone(),
            path_balance_constraints: self.path_balance_constraints.clone(),
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

        let mut pulse_edges = HashMap::new();
        for (from, _to, meta) in repr.pulse_edges {
            pulse_edges.insert(from, meta);
        }

        Ok(Self {
            nodes: repr.nodes,
            edges,
            driven_inputs,
            pulse_edges,
            clock_domains: repr.clock_domains,
            path_balance_constraints: repr.path_balance_constraints,
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
            clock_domain: None,
            macro_boundary: None,
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
        let mut pairs: Vec<(PinRef, PinRef)> =
            self.edges.iter().map(|(from, to)| (*from, *to)).collect();
        pairs.sort();
        pairs
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

    /// Removes nodes with no connections and reindexes remaining nodes.
    ///
    /// After compaction, [`NodeId`]s are reassigned contiguously from 0.
    /// This can significantly reduce memory for netlists with many
    /// disconnected placeholder nodes.
    ///
    /// Pulse-level metadata (pulse edges, clock domains, path balance
    /// constraints) is preserved and reindexed accordingly.
    pub fn compact(&mut self) {
        let node_count = self.nodes.len();
        let mut has_connection = vec![false; node_count];
        for (from, to) in &self.edges {
            has_connection[from.node.0] = true;
            has_connection[to.node.0] = true;
        }

        let mut old_to_new: Vec<Option<usize>> = vec![None; node_count];
        let mut kept = Vec::new();
        for (old_idx, node) in self.nodes.iter().enumerate() {
            if has_connection[old_idx] {
                old_to_new[old_idx] = Some(kept.len());
                kept.push(node.clone());
            }
        }

        for node in &mut kept {
            node.id = NodeId(old_to_new[node.id.0].unwrap());
        }

        let remap_pin = |pin: &PinRef, old_to_new: &[Option<usize>]| -> Option<PinRef> {
            old_to_new.get(pin.node.0).and_then(|v| v.as_ref()).map(|&new_idx| PinRef {
                node: NodeId(new_idx),
                port: pin.port,
            })
        };

        let mut new_edges = HashMap::new();
        let mut new_driven = HashSet::new();
        for (from, to) in &self.edges {
            if let (Some(new_from), Some(new_to)) = (remap_pin(from, &old_to_new), remap_pin(to, &old_to_new))
            {
                new_edges.insert(new_from, new_to);
                new_driven.insert(new_to);
            }
        }

        // Remap pulse_edges keys to new node ids.
        let mut new_pulse_edges = HashMap::new();
        for (from, meta) in &self.pulse_edges {
            if let Some(new_from) = remap_pin(from, &old_to_new) {
                new_pulse_edges.insert(new_from, meta.clone());
            }
        }

        // Remap pin references inside path balance constraints.
        let new_constraints: Vec<PathBalanceConstraint> = self
            .path_balance_constraints
            .iter()
            .filter_map(|c| {
                let a = remap_pin(&c.path_a_start, &old_to_new)?;
                let b = remap_pin(&c.path_b_start, &old_to_new)?;
                let conv = remap_pin(&c.convergence_point, &old_to_new)?;
                Some(PathBalanceConstraint {
                    name: c.name.clone(),
                    path_a_start: a,
                    path_b_start: b,
                    convergence_point: conv,
                    tolerance_ps: c.tolerance_ps,
                    clock_domain: c.clock_domain,
                })
            })
            .collect();

        self.nodes = kept;
        self.edges = new_edges;
        self.driven_inputs = new_driven;
        self.pulse_edges = new_pulse_edges;
        self.path_balance_constraints = new_constraints;
    }

    /// Estimates total memory usage in bytes.
    #[must_use]
    pub fn memory_usage_bytes(&self) -> usize {
        let nodes_size = self.nodes.len() * std::mem::size_of::<Node>();
        let edges_size = self.edges.len() * std::mem::size_of::<(PinRef, PinRef)>();
        let driven_size = self.driven_inputs.len() * std::mem::size_of::<PinRef>();
        let pulse_size = self.pulse_edges.len()
            * (std::mem::size_of::<PinRef>() + std::mem::size_of::<PulseEdge>());
        let cd_size = self.clock_domains.len() * std::mem::size_of::<ClockDomain>();
        let bc_size =
            self.path_balance_constraints.len() * std::mem::size_of::<PathBalanceConstraint>();
        nodes_size + edges_size + driven_size + pulse_size + cd_size + bc_size
    }

    // ------------------------------------------------------------------
    // Pulse-level API (P0-1)
    // ------------------------------------------------------------------

    /// Sets or replaces the [`PulseEdge`] metadata for an existing edge.
    ///
    /// The edge must already exist (created via [`connect`](Self::connect)).
    /// Returns `false` if no edge starts at `from`.
    pub fn set_pulse_edge(&mut self, from: PinRef, meta: PulseEdge) -> bool {
        if !self.edges.contains_key(&from) {
            return false;
        }
        self.pulse_edges.insert(from, meta);
        true
    }

    /// Returns a reference to the [`PulseEdge`] metadata for an edge,
    /// or `None` if the edge has no metadata or does not exist.
    #[must_use]
    pub fn pulse_edge(&self, from: PinRef) -> Option<&PulseEdge> {
        self.pulse_edges.get(&from)
    }

    /// Returns a mutable reference to the [`PulseEdge`] metadata for
    /// an edge.  Inserts a default entry if the edge exists but has
    /// no metadata yet.
    pub fn pulse_edge_mut(&mut self, from: PinRef) -> Option<&mut PulseEdge> {
        if !self.edges.contains_key(&from) {
            return None;
        }
        Some(self.pulse_edges.entry(from).or_default())
    }

    /// Removes pulse metadata for an edge.  Returns the removed
    /// metadata, if any.
    pub fn remove_pulse_edge(&mut self, from: PinRef) -> Option<PulseEdge> {
        self.pulse_edges.remove(&from)
    }

    /// All edges that have pulse metadata, as `(source, dest, meta)`.
    #[must_use]
    pub fn pulse_edge_triples(&self) -> Vec<(PinRef, PinRef, &PulseEdge)> {
        self.pulse_edges
            .iter()
            .filter_map(|(from, meta)| {
                self.edges.get(from).map(|to| (*from, *to, meta))
            })
            .collect()
    }

    // --- Clock domains ---

    /// Adds a new clock domain, returning its [`ClockDomainId`].
    pub fn add_clock_domain(&mut self, domain: ClockDomain) -> ClockDomainId {
        let id = ClockDomainId(self.clock_domains.len());
        let mut d = domain;
        d.id = id;
        self.clock_domains.push(d);
        id
    }

    /// Returns a reference to a clock domain by id.
    #[must_use]
    pub fn clock_domain(&self, id: ClockDomainId) -> Option<&ClockDomain> {
        self.clock_domains.get(id.0)
    }

    /// All clock domain definitions.
    #[must_use]
    pub fn clock_domains(&self) -> &[ClockDomain] {
        &self.clock_domains
    }

    /// Assigns a node to a clock domain.  Returns `false` if the node
    /// id is out of range.
    pub fn set_node_clock_domain(
        &mut self,
        node: NodeId,
        domain: Option<ClockDomainId>,
    ) -> bool {
        if let Some(n) = self.nodes.get_mut(node.0) {
            n.clock_domain = domain;
            true
        } else {
            false
        }
    }

    // --- Path balance constraints ---

    /// Adds a path balance constraint.
    pub fn add_path_balance_constraint(&mut self, constraint: PathBalanceConstraint) {
        self.path_balance_constraints.push(constraint);
    }

    /// All path balance constraints.
    #[must_use]
    pub fn path_balance_constraints(&self) -> &[PathBalanceConstraint] {
        &self.path_balance_constraints
    }

    /// Removes a path balance constraint by index.  Returns `None` if
    /// the index is out of range.
    pub fn remove_path_balance_constraint(&mut self, index: usize) -> Option<PathBalanceConstraint> {
        if index < self.path_balance_constraints.len() {
            Some(self.path_balance_constraints.remove(index))
        } else {
            None
        }
    }

    // --- Macro cell boundary ---

    /// Sets the macro-cell boundary annotation for a node.
    pub fn set_macro_boundary(&mut self, node: NodeId, boundary: MacroCellBoundary) -> bool {
        if let Some(n) = self.nodes.get_mut(node.0) {
            n.macro_boundary = Some(boundary);
            true
        } else {
            false
        }
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

    #[test]
    fn netlist_memory_usage() {
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "a".to_string());
        let usage = netlist.memory_usage_bytes();
        assert!(usage > 0);
    }

    #[test]
    fn compact_removes_disconnected_nodes() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        let _disconnected = netlist.add_node(NodeKind::CellInstance, "disc");
        let b = netlist.add_node(NodeKind::Port, "b");

        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .unwrap();

        assert_eq!(netlist.node_count(), 3);
        netlist.compact();
        assert_eq!(netlist.node_count(), 2);
        assert_eq!(netlist.edge_count(), 1);
    }

    #[test]
    fn compact_reindexes_contiguously() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::Port, "a");
        netlist.add_node(NodeKind::CellInstance, "disc1");
        netlist.add_node(NodeKind::CellInstance, "disc2");
        let b = netlist.add_node(NodeKind::Port, "b");

        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: b, port: 0 })
            .unwrap();

        netlist.compact();
        let nodes = netlist.nodes();
        assert_eq!(nodes[0].id.0, 0);
        assert_eq!(nodes[1].id.0, 1);
        assert_eq!(nodes[0].name, "a");
        assert_eq!(nodes[1].name, "b");
    }

    // --- Pulse-level IR tests (P0-1) ---

    #[test]
    fn pulse_window_basic() {
        let w = PulseWindow::new(10.0, 20.0).unwrap();
        assert!((w.width_ps() - 10.0).abs() < f64::EPSILON);
        assert!(w.overlaps(&PulseWindow::new(15.0, 25.0).unwrap()));
        assert!(!w.overlaps(&PulseWindow::new(21.0, 30.0).unwrap()));
    }

    #[test]
    fn pulse_window_invalid_returns_none() {
        assert!(PulseWindow::new(20.0, 10.0).is_none());
    }

    #[test]
    fn pulse_window_shift() {
        let mut w = PulseWindow::new(10.0, 20.0).unwrap();
        w.shift(5.0);
        assert!((w.earliest_ps - 15.0).abs() < f64::EPSILON);
        assert!((w.latest_ps - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pulse_window_display() {
        let w = PulseWindow::new(1.234, 5.678).unwrap();
        let s = format!("{w}");
        assert!(s.contains("1.23"));
        assert!(s.contains("5.68"));
    }

    #[test]
    fn set_and_get_pulse_edge() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let from = PinRef { node: a, port: 0 };
        let to = PinRef { node: b, port: 0 };
        netlist.connect(from, to).unwrap();

        let meta = PulseEdge {
            pulse_window: Some(PulseWindow::new(0.0, 5.0).unwrap()),
            delay_ps: 2.5,
            balance_critical: true,
            clock_domain: None,
        };
        assert!(netlist.set_pulse_edge(from, meta));

        let got = netlist.pulse_edge(from).unwrap();
        assert!((got.delay_ps - 2.5).abs() < f64::EPSILON);
        assert!(got.balance_critical);
        assert!(got.pulse_window.is_some());
    }

    #[test]
    fn set_pulse_edge_fails_for_missing_edge() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let fake = PinRef { node: a, port: 0 };
        assert!(!netlist.set_pulse_edge(fake, PulseEdge::default()));
    }

    #[test]
    fn pulse_edge_mut_inserts_default() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let from = PinRef { node: a, port: 0 };
        let to = PinRef { node: b, port: 0 };
        netlist.connect(from, to).unwrap();

        {
            let meta = netlist.pulse_edge_mut(from).unwrap();
            meta.delay_ps = 3.14;
        }
        assert!((netlist.pulse_edge(from).unwrap().delay_ps - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn remove_pulse_edge() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let from = PinRef { node: a, port: 0 };
        netlist.connect(from, PinRef { node: b, port: 0 }).unwrap();
        netlist.set_pulse_edge(from, PulseEdge::default());
        assert!(netlist.remove_pulse_edge(from).is_some());
        assert!(netlist.pulse_edge(from).is_none());
    }

    #[test]
    fn pulse_edge_triples() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let from = PinRef { node: a, port: 0 };
        let to = PinRef { node: b, port: 0 };
        netlist.connect(from, to).unwrap();
        netlist.set_pulse_edge(from, PulseEdge { delay_ps: 1.0, ..Default::default() });

        let triples = netlist.pulse_edge_triples();
        assert_eq!(triples.len(), 1);
        assert_eq!(triples[0].0, from);
        assert_eq!(triples[0].1, to);
        assert!((triples[0].2.delay_ps - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn clock_domain_round_trip() {
        let mut netlist = Netlist::new();
        let cd = netlist.add_clock_domain(ClockDomain {
            id: ClockDomainId(0),
            name: "clk_main".to_string(),
            frequency_ghz: 100.0,
            phase_rad: 0.0,
            phases: 2,
        });
        assert_eq!(cd.0, 0);
        let got = netlist.clock_domain(cd).unwrap();
        assert_eq!(got.name, "clk_main");
        assert!((got.frequency_ghz - 100.0).abs() < f64::EPSILON);
        assert_eq!(got.phases, 2);
    }

    #[test]
    fn node_clock_domain_assignment() {
        let mut netlist = Netlist::new();
        let cd = netlist.add_clock_domain(ClockDomain {
            id: ClockDomainId(0),
            name: "clk_a".to_string(),
            frequency_ghz: 50.0,
            phase_rad: 0.0,
            phases: 1,
        });
        let n = netlist.add_node(NodeKind::CellInstance, "gate1");
        assert!(netlist.set_node_clock_domain(n, Some(cd)));
        assert_eq!(netlist.nodes()[n.0].clock_domain, Some(cd));
    }

    #[test]
    fn path_balance_constraint_add_remove() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "src_a");
        let b = netlist.add_node(NodeKind::CellInstance, "src_b");
        let dff = netlist.add_node(NodeKind::Dff, "dff1");

        let c = PathBalanceConstraint {
            name: "test_balance".to_string(),
            path_a_start: PinRef { node: a, port: 0 },
            path_b_start: PinRef { node: b, port: 0 },
            convergence_point: PinRef { node: dff, port: 0 },
            tolerance_ps: 5.0,
            clock_domain: None,
        };
        netlist.add_path_balance_constraint(c);
        assert_eq!(netlist.path_balance_constraints().len(), 1);
        assert_eq!(netlist.path_balance_constraints()[0].name, "test_balance");

        let removed = netlist.remove_path_balance_constraint(0);
        assert!(removed.is_some());
        assert_eq!(netlist.path_balance_constraints().len(), 0);
    }

    #[test]
    fn macro_cell_boundary_annotation() {
        let mut netlist = Netlist::new();
        let mc = netlist.add_node(NodeKind::MacroCell, "macro1");
        let boundary = MacroCellBoundary {
            clock_pins: vec![PinRef { node: mc, port: 0 }],
            data_pins: vec![
                PinRef { node: mc, port: 1 },
                PinRef { node: mc, port: 2 },
            ],
        };
        assert!(netlist.set_macro_boundary(mc, boundary));
        let node = &netlist.nodes()[mc.0];
        let b = node.macro_boundary.as_ref().unwrap();
        assert_eq!(b.clock_pins.len(), 1);
        assert_eq!(b.data_pins.len(), 2);
    }

    #[test]
    fn compact_preserves_pulse_metadata() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let _disc = netlist.add_node(NodeKind::CellInstance, "disc");
        let b = netlist.add_node(NodeKind::CellInstance, "b");

        let from = PinRef { node: a, port: 0 };
        let to = PinRef { node: b, port: 0 };
        netlist.connect(from, to).unwrap();
        netlist.set_pulse_edge(from, PulseEdge { delay_ps: 7.7, ..Default::default() });

        netlist.compact();

        // a was node 0 → stays 0, b was node 2 → becomes 1
        let new_from = PinRef { node: NodeId(0), port: 0 };
        let meta = netlist.pulse_edge(new_from).unwrap();
        assert!((meta.delay_ps - 7.7).abs() < f64::EPSILON);
    }

    #[test]
    fn compact_preserves_path_balance_constraints() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let _disc = netlist.add_node(NodeKind::CellInstance, "disc");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let c = netlist.add_node(NodeKind::Dff, "dff");

        // Connect so a, b, c have connections.
        netlist
            .connect(PinRef { node: a, port: 0 }, PinRef { node: c, port: 0 })
            .unwrap();
        netlist
            .connect(PinRef { node: b, port: 0 }, PinRef { node: c, port: 1 })
            .unwrap();

        netlist.add_path_balance_constraint(PathBalanceConstraint {
            name: "bal".to_string(),
            path_a_start: PinRef { node: a, port: 0 },
            path_b_start: PinRef { node: b, port: 0 },
            convergence_point: PinRef { node: c, port: 0 },
            tolerance_ps: 3.0,
            clock_domain: None,
        });

        netlist.compact();

        let constraints = netlist.path_balance_constraints();
        assert_eq!(constraints.len(), 1);
        // a=0, b=2→1, c=3→2
        assert_eq!(constraints[0].path_a_start.node, NodeId(0));
        assert_eq!(constraints[0].path_b_start.node, NodeId(1));
        assert_eq!(constraints[0].convergence_point.node, NodeId(2));
    }

    #[test]
    fn json_round_trip_with_pulse_metadata() {
        let mut netlist = Netlist::new();
        let cd = netlist.add_clock_domain(ClockDomain {
            id: ClockDomainId(0),
            name: "clk".to_string(),
            frequency_ghz: 80.0,
            phase_rad: 1.57,
            phases: 2,
        });
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let from = PinRef { node: a, port: 0 };
        let to = PinRef { node: b, port: 0 };
        netlist.connect(from, to).unwrap();
        netlist.set_node_clock_domain(a, Some(cd));
        netlist.set_pulse_edge(
            from,
            PulseEdge {
                pulse_window: Some(PulseWindow::new(0.0, 10.0).unwrap()),
                delay_ps: 4.2,
                balance_critical: true,
                clock_domain: Some(cd),
            },
        );
        netlist.add_path_balance_constraint(PathBalanceConstraint {
            name: "test".to_string(),
            path_a_start: from,
            path_b_start: from,
            convergence_point: to,
            tolerance_ps: 5.0,
            clock_domain: Some(cd),
        });

        let json = serde_json::to_string(&netlist).unwrap();
        let restored: Netlist = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.clock_domains().len(), 1);
        assert_eq!(restored.clock_domains()[0].name, "clk");
        assert!((restored.pulse_edge(from).unwrap().delay_ps - 4.2).abs() < f64::EPSILON);
        assert_eq!(restored.path_balance_constraints().len(), 1);
        assert_eq!(restored.nodes()[a.0].clock_domain, Some(cd));
    }

    #[test]
    fn json_backward_compatible_no_pulse_fields() {
        // Simulate a JSON from old version without pulse fields.
        let json = r#"{"nodes":[{"id":0,"kind":"Port","name":"x"},{"id":1,"kind":"Port","name":"y"}],"edges":[[[0,0],[1,0]]]}"#;
        let netlist: Netlist = serde_json::from_str(json).unwrap();
        assert_eq!(netlist.node_count(), 2);
        assert!(netlist.pulse_edge(PinRef { node: NodeId(0), port: 0 }).is_none());
        assert!(netlist.clock_domains().is_empty());
        assert!(netlist.path_balance_constraints().is_empty());
    }

    #[test]
    fn memory_usage_includes_pulse_metadata() {
        let mut netlist = Netlist::new();
        let a = netlist.add_node(NodeKind::CellInstance, "a");
        let b = netlist.add_node(NodeKind::CellInstance, "b");
        let from = PinRef { node: a, port: 0 };
        netlist.connect(from, PinRef { node: b, port: 0 }).unwrap();
        netlist.set_pulse_edge(from, PulseEdge::default());
        netlist.add_clock_domain(ClockDomain {
            id: ClockDomainId(0),
            name: "clk".to_string(),
            frequency_ghz: 100.0,
            phase_rad: 0.0,
            phases: 1,
        });

        let usage = netlist.memory_usage_bytes();
        assert!(usage > 0);
        // Should be larger than a plain netlist without pulse metadata.
        let mut plain = Netlist::new();
        plain.add_node(NodeKind::CellInstance, "a");
        plain.add_node(NodeKind::CellInstance, "b");
        assert!(usage > plain.memory_usage_bytes());
    }
}
