# Yosys Alignment Matrix

Date: 2026-05-21

## Scope

This document maps the current `rflux` workspace to the roles commonly associated with Yosys in a digital design flow, and also clarifies where `rflux` already extends into a broader SFQ EDA implementation stack.

The goal is not to claim full feature parity with Yosys. The goal is to answer two narrower questions:

- which current `rflux` modules are the closest conceptual counterparts to Yosys
- which stages of an SFQ EDA toolchain the current repository can already cover in executable form
- which modules sit outside normal Yosys scope and are closer to P&R, STA, or simulation tools
- where the current implementation is still narrower than a mature Yosys frontend/mid-end

## Summary

The modules that most directly align with Yosys are:

- `rflux-ir`
- `rflux-synth`
- `rflux-verify`
- `rflux-sat`
- `rflux-cli`

These modules collectively cover the same broad territory as a Yosys-style frontend plus synthesis, SAT, equivalence, and CLI driver stack, but with an SFQ-oriented IR and SFQ-specific transformations.

The rest of the workspace extends beyond normal Yosys responsibility:

- `rflux-place` and `rflux-route` are closer to nextpnr/OpenROAD-style physical implementation
- `rflux-timing` is closer to OpenSTA-style timing analysis
- `rflux-sim` is a simulation layer, closer to a dedicated circuit or event simulator than to Yosys
- `rflux-flow` is an orchestration layer spanning synthesis through physical and verification stages

In other words, `rflux` should not be described only as a Yosys analog. It is better described as an SFQ-oriented implementation stack whose Yosys-like portion sits mainly in IR, synthesis, SAT, equivalence, and CLI flow driving.

## SFQ EDA flow position

If the question is not "what aligns with Yosys" but instead "which stages of an SFQ EDA toolchain can the current project cover", the answer is broader than synthesis alone.

The current workspace spans most of the middle of an SFQ-oriented digital implementation flow, plus parts of verification and simulation:

| SFQ EDA stage | Current `rflux` modules | Current status | Notes |
|---|---|---|---|
| Design representation / internal modeling | `rflux-ir`, `rflux-hdl`, `rflux-io` | implemented | Provides an SFQ-oriented IR, a minimal Rust-side builder DSL, and import/export paths centered on IR JSON plus some exchange formats. |
| SFQ logic synthesis | `rflux-synth`, `rflux-tech` | implemented | Covers splitter insertion, path-balancing DFF insertion, boolean optimization, and minimal technology mapping. |
| Flow orchestration | `rflux-flow`, `rflux-cli`, `rflux-py` / `python/rflux` | implemented | Provides the end-to-end driver layer that connects synthesis, layout, timing, verification, and simulation-facing entrypoints. |
| Placement | `rflux-place` | implemented prototype | Includes levelized placement, fixed-node handling, blocked regions, halo support, and simple congestion-aware spilling. |
| Routing | `rflux-route` | implemented prototype | Includes JTL/PTL mixed routing, boundary-aware routing, keep-out detours, and routing detour metrics. |
| Static timing analysis | `rflux-timing` | implemented prototype | Covers deterministic STA plus the current SSTA subset, including node, pin, and clock-domain constraints. |
| Equivalence and structural verification | `rflux-verify`, `rflux-sat` | implemented | Covers combinational SAT equivalence, a narrow single-step sequential equivalence subset, DIMACS export/import, assumptions, and UNSAT-core workflows. |
| Simulation | `rflux-sim`, `rflux-flow` | implemented but still maturing | Supports event-only, external JoSIM, and an internal transient subset, but is not yet a full JoSIM-class native simulator. |
| Library characterization and feedback optimization | `rflux-flow`, `rflux-tech`, `rflux-timing` | implemented prototype | Phase 5 work already feeds characterization artifacts back into timing and design-space optimization loops. |
| Physical signoff, full DRC/LVS, mature GDS handoff | none yet | not implemented | This remains outside the current repository's completed capability envelope. |

### Compact flow picture

The current project is best understood as the following SFQ EDA slice:

```text
design entry / IR
	-> SFQ synthesis
	-> placement
	-> routing
	-> timing analysis
	-> equivalence / structural verification
	-> simulation / correlation

with `rflux-flow` orchestrating the middle stages
```

### What this means in practice

`rflux` is not just a Yosys-like synthesis core.

It is closer to an SFQ-oriented implementation stack with these characteristics:

- front-end and internal representation for SFQ-specific constructs
- synthesis aware of splitter and path-balance constraints
- prototype physical implementation through placement and routing
- prototype STA and SSTA
- formal-equivalence and SAT utilities
- mixed verification/simulation paths through event-only, external-simulator, and internal-transient modes

### Current boundary

The repository should not yet be described as a full end-to-end SFQ signoff platform.

That boundary is about current implementation status, not about long-term ambition.
The long-term end-state described by the project direction is still a signoff-class, end-to-end SFQ EDA platform covering the full path from SFQ-oriented design entry and synthesis through physical implementation, timing, verification, simulation, and eventual manufacturing handoff.

What is already credible to claim:

- an executable research-oriented SFQ EDA prototype
- coverage from internal representation and synthesis into P&R, timing, verification, and part of simulation
- Python and CLI entrypoints for end-to-end experiments

What would overstate the current implementation:

- full mature Verilog-front-end parity with Yosys
- full industrial physical signoff coverage
- full JoSIM-class native analog simulation parity
- a complete manufacturing tapeout stack

## Chinese summary

如果从更符合仓库语境的中文表述来概括，这个项目在 SFQ EDA 工具链中的位置可以归纳为：

- 前端表示层：`rflux-ir`、`rflux-hdl`、`rflux-io` 提供 SFQ 电路的内部表示、最小建模入口和交换路径。
- 综合层：`rflux-synth` 已覆盖 splitter 自动插入、路径平衡 DFF 插入、布尔优化和最小技术映射。
- 物理实现层：`rflux-place` 与 `rflux-route` 已形成可执行的布局布线原型。
- 时序分析层：`rflux-timing` 已提供确定性 STA 和当前范围内的 SSTA。
- 验证层：`rflux-verify` 与 `rflux-sat` 已支持组合等价、部分单步顺序等价、DIMACS 导出回放与假设求解。
- 仿真层：`rflux-sim` 已支持 event-only、external JoSIM 和 internal transient 三类路径，但原生模拟能力仍在持续补齐。
- 编排与脚本层：`rflux-flow`、`rflux-cli`、`rflux-py` / `python/rflux` 将这些阶段串成可运行的实验流。

因此，`rflux` 当前更适合被称为“面向 SFQ 的研究型 EDA 原型工具链”，而不是单一的综合器，或者已经完成 signoff 的工业全流程平台。

同时，这里的“原型工具链”描述针对的是当前状态，而不是终极目标。项目的长期目标仍然是朝着 SFQ 器件设计所需的 signoff 级全流程平台推进，只是现阶段尚未达到这一完成度。

## Tool-family comparison

The easiest way to position `rflux` is not to ask whether it equals one mature tool, but to ask which part of its current workspace overlaps with which established tool family.

| Tool family | Closest `rflux` modules | Relationship |
|---|---|---|
| Yosys | `rflux-ir`, `rflux-synth`, `rflux-verify`, `rflux-sat`, `rflux-cli` | This is the closest conceptual overlap: IR, synthesis, SAT, equivalence, and command-line flow entry. |
| OpenROAD / nextpnr | `rflux-place`, `rflux-route`, parts of `rflux-flow` | `rflux` already extends into physical implementation, but in a narrower SFQ-specific prototype form rather than a mature general-purpose P&R stack. |
| OpenSTA | `rflux-timing`, parts of `rflux-flow` | `rflux` has explicit timing analysis responsibility, including SFQ-oriented timing and lightweight statistical analysis. |
| JoSIM / SPICE-style transient simulation | `rflux-sim`, `rflux-flow` | `rflux` includes simulation backends and JoSIM-facing integration, but native parity is still partial and deliberately scoped. |
| Python workflow glue around EDA tools | `rflux-py`, `python/rflux`, `python/scripts` | This is the scripting and experiment layer that makes the Rust crates usable in notebooks, scripts, and batch workflows. |

From that perspective, `rflux` is not a clone of Yosys, OpenROAD, OpenSTA, or JoSIM.
It is a repository that currently combines a Yosys-like synthesis and verification core with prototype physical, timing, and simulation layers tailored to SFQ constraints.

## Alignment matrix

| `rflux` module | Closest Yosys role | Current alignment | Notes |
|---|---|---|---|
| `rflux-ir` | Internal design IR such as RTLIL/netlist core | strong | Owns `NodeKind`, `LogicOp`, and `Netlist`; this is the shared representation consumed by synth and verification. |
| `rflux-synth` | Core synthesis passes, optimization, mapping, SAT-backed equivalence helpers | strong | Owns compile planning, splitter insertion, path-balance DFF insertion, boolean optimization, tech mapping, and SAT-based equivalence export/check helpers. |
| `rflux-verify` | `equiv_*` and verification-facing wrappers | strong | Provides a verification-focused facade over combinational and single-step sequential equivalence checks. |
| `rflux-sat` | `sat` backend / CNF solving support | strong | Provides CNF, DIMACS import/export, incremental solving, assumptions, metrics, and UNSAT core extraction. |
| `rflux-cli` | Yosys command-line entrypoint | strong | Exposes `compile-netlist`, `check-equivalence`, and `solve-dimacs`, which are the most Yosys-like user-facing flows in the current repo. |
| `rflux-hdl` | Alternative frontend / DSL input layer | partial | Currently a minimal Rust builder DSL, not a Verilog frontend or a general HDL lowering stack. |
| `rflux-io` | Frontend/backend file I/O layer | partial | Currently strongest on IR JSON, a minimal `.bench` gate-level subset, and LEF/DEF exchange; it is not yet equivalent to a mature `read_verilog`/`write_blif` style frontend/backend set. |
| `rflux-tech` | Library and technology data used by mapping passes | partial | Supports technology-aware mapping and physical/timing consumers, but it is broader than a pure Yosys liberty/techmap role because it also feeds layout/timing phases. |
| `rflux-flow` | Flow script or external orchestration around Yosys | weak | This is broader than Yosys itself because it coordinates synthesis, place, route, timing, verification, and simulation-facing hooks. |
| `rflux-place` | none inside core Yosys | none | Physical placement is outside normal Yosys scope. |
| `rflux-route` | none inside core Yosys | none | Physical routing is outside normal Yosys scope. |
| `rflux-timing` | none inside core Yosys | none | Static timing analysis is typically handled by a separate tool. |
| `rflux-sim` | none inside core Yosys | none | Simulation is a separate tool category. |
| `rflux-py` / `python/rflux` | external scripting/binding layer around Yosys APIs | weak | Useful as workflow glue, but not a direct analog to Yosys internals. |

## Module-by-module notes

### `rflux-ir`

This crate is the clearest internal-IR analog.

Why it aligns:

- it defines the workspace netlist representation
- it centralizes node kinds and logic operations
- downstream synthesis and verification crates consume it as the common graph form

Current SFQ-specific difference from a Yosys-style IR:

- the IR directly models `Splitter`, `Dff`, `Jtl`, and `Ptl`
- single-consumer connectivity is enforced in the base netlist API, reflecting SFQ fanout constraints rather than generic CMOS semantics

Reference anchors:

- `crates/ir/src/lib.rs`: `NodeKind`, `LogicOp`, `Netlist`

### `rflux-synth`

This crate is the strongest single Yosys counterpart in the repository.

Why it aligns:

- it owns synthesis-time graph rewrites
- it performs boolean optimization
- it performs technology mapping
- it exposes SAT-backed equivalence helpers and DIMACS-exportable equivalence problems

Current SFQ-specific difference from a Yosys-style synth engine:

- splitter insertion is a first-class synthesis concern
- path-balancing DFF insertion is built into the synthesis story
- optimization and mapping are constrained by an SFQ-oriented IR rather than an RTL frontend lowered into RTLIL

Reference anchors:

- `crates/synth/src/lib.rs`: `Compiler`, `compile_plan`, `TechMapper`, `check_boolean_equivalence_sat`, `build_boolean_equivalence_problem`

### `rflux-verify`

This crate maps cleanly to the user-facing side of Yosys equivalence flows.

Why it aligns:

- it exposes combinational equivalence checking
- it exposes single-step sequential equivalence for the current `Dff`/`DffEnable` subset
- it exports equivalence SAT problems for replayable external solving flows

Difference from mature Yosys equivalence tooling:

- the sequential scope is intentionally narrow at present
- matching is scoped to the currently supported state element subset rather than full sequential proof workflows

Reference anchors:

- `crates/verify/src/lib.rs`: `Verifier`, `check_boolean_equivalence`, `check_single_step_sequential_equivalence`, `build_boolean_equivalence_problem`

### `rflux-sat`

This crate is the closest equivalent to a built-in SAT backend and DIMACS utility layer.

Why it aligns:

- it owns CNF construction and DIMACS parsing/rendering
- it supports incremental solving with assumptions
- it reports solve metrics and supports UNSAT core extraction for assumption sets

Difference from Yosys ecosystem practice:

- this repo ships its own solver rather than delegating to an external SAT engine
- the scope is focused on the current equivalence and DIMACS workflows rather than every proof mode Yosys can drive

Reference anchors:

- `crates/sat/src/lib.rs`: `CnfFormula`, `from_dimacs`, `IncrementalSolver`, `unsat_core_of_assumptions`

### `rflux-cli`

This crate is the most directly visible Yosys-like entrypoint for end users.

Why it aligns:

- it exposes synthesis-style compilation commands
- it exposes equivalence checking commands
- it exposes DIMACS solving and equivalence replay commands

Difference from Yosys CLI today:

- the input format is currently centered on `rflux` IR JSON plus a minimal `.bench` gate-level subset rather than Verilog scripts
- the command set spans beyond synthesis into layout, timing, and simulation-oriented flows

Reference anchors:

- `crates/cli/src/main.rs`: `compile-netlist`, `check-equivalence`, `solve-dimacs`, `compile-layout`, `analyze-timing`, `verify-layout`

### `rflux-hdl`

This crate is only a partial frontend analog today.

Why it partially aligns:

- it constructs netlists from a user-facing builder API
- it serves as an input path into the shared IR

Why it does not yet strongly align:

- it is not a Verilog parser
- it is not a rich HDL lowering pipeline
- it is best understood today as a minimal Rust DSL/builder rather than a general frontend comparable to Yosys frontends

### `rflux-io`

This crate is also only a partial frontend/backend analog today.

Why it partially aligns:

- it owns import/export responsibilities
- it handles IR JSON, a minimal `.bench` gate-level subset, and LEF/DEF conversion paths

Why it remains narrower than Yosys frontend/backend coverage:

- current implemented strength is IR JSON plus a minimal `.bench` subset and LEF/DEF exchange
- the `.bench` path is limited to a small Quaigh-style subset (`AND`/`OR`/`XOR`/`XNOR`/`NOT`/`NAND`/`NOR`/`BUF`/`MUX`/`DFF`/`DFFE`/`MAJ`/`AOI21`/`OAI21`/`AOI22`/`OAI22`/`AOI31`/`OAI31`/`AOI211`/`OAI211`/`AOI311`/`OAI311`/`AOI321`/`OAI321`/`AOI221`/`OAI221`/`AOI222`/`OAI222`/`AOI322`/`OAI322`/`AOI421`/`OAI421`/`AOI422`/`OAI422`/`AOI431`/`OAI431`/`AOI432`/`OAI432`/`AOI433`/`OAI433`/`AOI441`/`OAI441`/`AOI442`/`OAI442`/`AOI443`/`OAI443`/`AOI444`/`OAI444`/`AOI2221`/`OAI2221` plus ports, with `XNOR`/`NAND`/`NOR`/`DFF`/`DFFE`/`MAJ`/`AOI21`/`OAI21`/`AOI22`/`OAI22`/`AOI31`/`OAI31`/`AOI211`/`OAI211`/`AOI311`/`OAI311`/`AOI321`/`OAI321`/`AOI221`/`OAI221`/`AOI222`/`OAI222`/`AOI322`/`OAI322`/`AOI421`/`OAI421`/`AOI422`/`OAI422`/`AOI431`/`OAI431`/`AOI432`/`OAI432`/`AOI433`/`OAI433`/`AOI441`/`OAI441`/`AOI442`/`OAI442`/`AOI443`/`OAI443`/`AOI444`/`OAI444`/`AOI2221`/`OAI2221` lowered into the existing IR, the current signal-token parser accepts bit-style names such as `a[0]`, duplicate `INPUT/OUTPUT` declarations and gate redefinitions of INPUT names are explicitly rejected, `INPUT(name)` plus `OUTPUT(name)` passthrough remains supported, acyclic gate definitions may appear before their consumers, checked-in sequential bench fixtures now live in a dedicated `bench_sequential/` lane with `dff_basic`, `dffe_basic`, and a checked-in `dff_dffe_mismatch` pair, and the CLI has explicit single-step sequential bench-equivalence regressions for matching cases, mismatching cases, and diagnostics-bundle mismatch reporting in that lane rather than mixing it into the combinational bench-equivalence fixture set) rather than a general frontend
- the repo does not currently expose a mature Verilog frontend comparable to `read_verilog`

## Practical takeaway

If the question is "which crates in `rflux` should I compare against Yosys first", start with:

1. `rflux-ir`
2. `rflux-synth`
3. `rflux-verify`
4. `rflux-sat`
5. `rflux-cli`

If the question is "which crates go beyond Yosys and start looking like a full physical-design and analysis stack", look next at:

1. `rflux-flow`
2. `rflux-place`
3. `rflux-route`
4. `rflux-timing`
5. `rflux-sim`

## References used

- `docs/project-design.md`
- `README.md`
- `crates/ir/src/lib.rs`
- `crates/hdl/src/lib.rs`
- `crates/io/src/lib.rs`
- `crates/synth/src/lib.rs`
- `crates/verify/src/lib.rs`
- `crates/sat/src/lib.rs`
- `crates/cli/src/main.rs`