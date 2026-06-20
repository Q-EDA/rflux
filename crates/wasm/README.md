# rflux-wasm

WebAssembly bindings for rflux, the Rust-based SFQ EDA toolkit.

## Installation

```bash
npm install @rflux/eda
```

## Usage

### Basic Usage

```javascript
import init, { version, Circuit, example_circuit_json, compile_json } from '@rflux/eda';

// Initialize the module
await init();

console.log('rflux version:', version());

// Create a circuit programmatically
const circuit = new Circuit();
const a = circuit.add_node('port', 'a');
const b = circuit.add_node('port', 'b');
const xor = circuit.add_node('cell', 'xor0', 'xor');
const out = circuit.add_node('port', 'out');

circuit.connect(a, 0, xor, 0);
circuit.connect(b, 0, xor, 1);
circuit.connect(xor, 0, out, 0);

const circuitJson = circuit.to_json();
console.log('Circuit:', JSON.parse(circuitJson));

// Or load the example circuit
const exampleJson = example_circuit_json();
console.log('Example:', JSON.parse(exampleJson));

// Compile to layout
const compileOptions = { clock_period_ps: 120 };
const result = compile_json(circuitJson, JSON.stringify(compileOptions));
console.log('Compile result:', JSON.parse(result));
```

### From JSON

```javascript
import init, { Circuit, compile_json } from '@rflux/eda';

await init();

// Load a circuit from JSON
const netlistJson = `
{
  "nodes": [ ... ],
  "edges": [ ... ]
}
`;

const resultJson = compile_json(netlistJson);
const result = JSON.parse(resultJson);

if (result.success) {
  console.log('Placement:', result.placement);
  console.log('Routing:', result.routing);
  console.log('Timing:', result.timing);
} else {
  console.error('Error:', result.error);
}
```

## Building from Source

### Prerequisites

- Rust (latest stable)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

### Build

```bash
# Development build
wasm-pack build --dev

# Release build (optimized for size)
wasm-pack build --release
```

### Test

```bash
# Run Rust tests
cargo test

# Run wasm tests in browser/Node.js
wasm-pack test --node
# or
wasm-pack test --chrome
```

### NPM Package

The built package in `pkg/` can be published to npm:

```bash
cd pkg
npm publish --access public
```

## API Reference

### `version()`

Returns the rflux version string.

### `init()`

Initializes the WebAssembly module. Call this first.

### `Circuit`

Class for building SFQ circuits.

- `constructor()`: Create a new empty circuit
- `add_node(kind, name, logic_op)`: Add a node
  - `kind`: "port" | "cell_instance" | "macro_cell" | "splitter" | "dff" | "jtl" | "ptl"
  - `name`: Node name
  - `logic_op`: Optional logic operation ("buf" | "not" | "and" | "or" | "xor" | "mux2" | "dffenable")
- `connect(from_node, from_port, to_node, to_port)`: Connect two nodes
- `node_count()`: Get number of nodes
- `edge_count()`: Get number of edges
- `to_json()`: Serialize circuit to JSON
- `from_json(json)`: Deserialize circuit from JSON (static method)

### `example_circuit_json()`

Returns a simple example XOR circuit as JSON string.

### `compile_json(circuit_json, options_json?)`

Compiles a circuit from JSON to layout.

- `circuit_json`: Netlist JSON as string
- `options_json`: Optional compile options as JSON string
  - `clock_period_ps`: Clock period in picoseconds (default: 120)

Returns a JSON string with compile result:

```typescript
{
  success: boolean;
  error?: string;
  placement?: {
    placed_nodes: number;
    width_um: number;
    height_um: number;
  };
  routing?: {
    routed_nets: number;
    total_length_um: number;
    jtl_routes: number;
    ptl_routes: number;
  };
  timing?: {
    worst_setup_slack_ps: number;
    worst_hold_slack_ps: number;
    critical_path_delay_ps: number;
    timing_closed: boolean;
  };
}
```

## License

MIT OR Apache-2.0
