use rflux_ir::{IrError, LogicOp, Netlist, NodeId, NodeKind, PinRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signal {
    pub pin: PinRef,
}

pub struct CircuitBuilder {
    netlist: Netlist,
}

impl CircuitBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            netlist: Netlist::new(),
        }
    }

    pub fn port(&mut self, name: impl Into<String>) -> Signal {
        let node = self.netlist.add_node(NodeKind::Port, name);
        Self::signal(node)
    }

    pub fn cell(&mut self, name: impl Into<String>) -> Signal {
        let node = self.netlist.add_node(NodeKind::CellInstance, name);
        Self::signal(node)
    }

    pub fn logic_cell(&mut self, name: impl Into<String>, logic_op: LogicOp) -> Signal {
        let node = self
            .netlist
            .add_node_with_logic(NodeKind::CellInstance, name, Some(logic_op));
        Self::signal(node)
    }

    pub fn macro_cell(&mut self, name: impl Into<String>) -> Signal {
        let node = self.netlist.add_node(NodeKind::MacroCell, name);
        Self::signal(node)
    }

    pub fn dff(&mut self, name: impl Into<String>) -> Signal {
        let node = self.netlist.add_node(NodeKind::Dff, name);
        Self::signal(node)
    }

    pub fn splitter(&mut self, name: impl Into<String>) -> Signal {
        let node = self.netlist.add_node(NodeKind::Splitter, name);
        Self::signal(node)
    }

    pub fn connect(&mut self, from: Signal, to: Signal) -> Result<&mut Self, IrError> {
        self.netlist.connect(from.pin, to.pin)?;
        Ok(self)
    }
    #[must_use]
    pub fn add_port(mut self, name: impl Into<String>) -> Self {
        self.port(name);
        self
    }

    #[must_use]
    pub fn finish(self) -> Netlist {
        self.netlist
    }

    fn signal(node: NodeId) -> Signal {
        Signal {
            pin: PinRef { node, port: 0 },
        }
    }
}

impl Default for CircuitBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_simple_connected_pipeline() {
        let mut builder = CircuitBuilder::new();
        let input = builder.port("in");
        let gate = builder.logic_cell("xor0", LogicOp::Xor);
        let stage = builder.dff("stage0");
        let output = builder.port("out");

        builder
            .connect(input, gate)
            .expect("input to gate")
            .connect(gate, stage)
            .expect("gate to stage")
            .connect(stage, output)
            .expect("stage to output");

        let netlist = builder.finish();
        assert_eq!(netlist.node_count(), 4);
        assert_eq!(netlist.edge_count(), 3);
        assert!(matches!(netlist.nodes()[1].logic_op, Some(LogicOp::Xor)));
    }

    #[test]
    fn supports_macro_and_splitter_nodes() {
        let mut builder = CircuitBuilder::new();
        let source = builder.port("src");
        let splitter = builder.splitter("split0");
        let macro_cell = builder.macro_cell("macro0");

        builder
            .connect(source, splitter)
            .expect("source to splitter")
            .connect(splitter, macro_cell)
            .expect("splitter to macro");

        let netlist = builder.finish();
        assert!(matches!(netlist.nodes()[1].kind, NodeKind::Splitter));
        assert!(matches!(netlist.nodes()[2].kind, NodeKind::MacroCell));
    }

    #[test]
    fn default_provides_empty_netlist() {
        let builder = CircuitBuilder::default();
        let netlist = builder.finish();
        assert_eq!(netlist.node_count(), 0);
        assert_eq!(netlist.edge_count(), 0);
    }

    #[test]
    fn add_port_chains_fluently() {
        let builder = CircuitBuilder::new()
            .add_port("a")
            .add_port("b")
            .add_port("c");
        let netlist = builder.finish();
        assert_eq!(netlist.node_count(), 3);
        assert!(netlist.nodes().iter().all(|n| matches!(n.kind, NodeKind::Port)));
    }

    #[test]
    fn signal_pins_to_first_port_of_node() {
        let mut builder = CircuitBuilder::new();
        let sig = builder.port("p");
        assert_eq!(sig.pin.port, 0);
        let netlist = builder.finish();
        assert_eq!(netlist.nodes()[0].name, "p");
    }

    #[test]
    fn all_logic_op_variants_build() {
        let mut builder = CircuitBuilder::new();
        let _and_cell = builder.logic_cell("and0", LogicOp::And);
        let _or_cell = builder.logic_cell("or0", LogicOp::Or);
        let _xor_cell = builder.logic_cell("xor0", LogicOp::Xor);
        let _not_cell = builder.logic_cell("not0", LogicOp::Not);
        let out = builder.port("o");
        builder.connect(_and_cell, out).unwrap();
        let netlist = builder.finish();
        assert!(netlist.nodes()[0].logic_op.is_some());
        assert!(netlist.nodes()[1].logic_op.is_some());
        assert!(netlist.nodes()[2].logic_op.is_some());
        assert!(netlist.nodes()[3].logic_op.is_some());
    }

    #[test]
    fn rejects_reconnect_to_same_target() {
        let mut builder = CircuitBuilder::new();
        let a = builder.port("a");
        let b = builder.cell("b");
        let c = builder.cell("c");
        builder.connect(a, b).unwrap();
        let result = builder.connect(a, c);
        assert!(matches!(result, Err(IrError::SourceAlreadyConnected)));
    }

    #[test]
    fn splitter_allows_fanout() {
        let mut builder = CircuitBuilder::new();
        let src = builder.port("src");
        let split = builder.splitter("s0");
        let a = builder.cell("a");
        let b = builder.cell("b");
        builder.connect(src, split).unwrap();
        builder
            .connect(
                Signal {
                    pin: PinRef {
                        node: split.pin.node,
                        port: 0,
                    },
                },
                a,
            )
            .unwrap();
        builder
            .connect(
                Signal {
                    pin: PinRef {
                        node: split.pin.node,
                        port: 1,
                    },
                },
                b,
            )
            .unwrap();
        let netlist = builder.finish();
        assert_eq!(netlist.edge_count(), 3);
    }

    #[test]
    fn build_complex_subcircuit() {
        let mut builder = CircuitBuilder::new();
        let clk = builder.port("clk");
        let din = builder.port("din");
        let xor0 = builder.logic_cell("xor0", LogicOp::Xor);
        let dff0 = builder.dff("dff0");
        let split = builder.splitter("s0");
        let mac = builder.macro_cell("mac0");
        let out = builder.port("out");

        builder
            .connect(din, xor0)
            .unwrap()
            .connect(xor0, dff0)
            .unwrap()
            .connect(
                clk,
                Signal {
                    pin: PinRef {
                        node: dff0.pin.node,
                        port: 1,
                    },
                },
            )
            .unwrap()
            .connect(dff0, split)
            .unwrap()
            .connect(split, mac)
            .unwrap()
            .connect(mac, out)
            .unwrap();

        let netlist = builder.finish();
        assert_eq!(netlist.node_count(), 7);
        assert_eq!(netlist.edge_count(), 6);
    }
}
