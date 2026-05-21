use rflux_ir::{IrError, LogicOp, Netlist, NodeId, NodeKind, PinRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signal {
    pub pin: PinRef,
}

pub struct CircuitBuilder {
    netlist: Netlist,
}

impl CircuitBuilder {
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

    pub fn add_port(mut self, name: impl Into<String>) -> Self {
        self.port(name);
        self
    }

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
}
