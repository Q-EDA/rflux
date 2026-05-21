use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use rflux_io::{read_ir_json, write_ir_json};
use rflux_ir::{Netlist, NodeKind, PinRef};
use rflux_synth::Compiler;

#[test]
fn json_ir_roundtrip_with_synth_pass() {
    let mut netlist = Netlist::new();
    let src = netlist.add_node(NodeKind::CellInstance, "src");
    let sink = netlist.add_node(NodeKind::CellInstance, "sink");

    let src_out = PinRef { node: src, port: 0 };
    let sink_in = PinRef {
        node: sink,
        port: 0,
    };

    netlist
        .connect(src_out, sink_in)
        .expect("initial edge should connect");

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();

    let input_path = env::temp_dir().join(format!("rflux_ir_in_{unique}.json"));
    let output_path = env::temp_dir().join(format!("rflux_ir_out_{unique}.json"));

    write_ir_json(&input_path, &netlist).expect("input json should be written");

    let mut loaded = read_ir_json(&input_path).expect("input json should be readable");
    let mut compiler = Compiler::new();
    compiler
        .insert_balancing_dff(&mut loaded, src_out)
        .expect("balancing dff insertion should succeed");

    write_ir_json(&output_path, &loaded).expect("output json should be written");
    let reloaded = read_ir_json(&output_path).expect("output json should be readable");

    assert_eq!(reloaded.node_count(), 3);
    assert_eq!(reloaded.edge_count(), 2);

    let _ = std::fs::remove_file(&input_path);
    let _ = std::fs::remove_file(&output_path);
}
