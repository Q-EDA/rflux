# rflux

Rust-first SFQ EDA toolkit for superconducting single-flux-quantum circuits.

`rflux` 是一个以 Rust 为核心、以 Python 为外层胶水与调用接口的 SFQ EDA 原型项目。它面向超导单磁通量子电路的建模、综合、布局布线、时序分析与验证，重点围绕 SFQ 特有的约束展开，例如显式 splitter 扇出、路径平衡、JTL/PTL 混合互连、多时钟域约束，以及面向 AC bias 的粗粒度设计评估。

当前仓库状态更接近“可执行的研究型原型”而不是完整产品：核心 Rust workspace 和 Python facade 已经联通，部分设计阶段已有可运行实现与测试覆盖，CLI 入口仍是占位状态。

## 项目目标

- 以 Rust 构建可移植、可测试、尽量保持 `wasm32-unknown-unknown` 兼容的 SFQ EDA 核心。
- 用显式 IR 表达 SFQ 电路中的脉冲语义、路径平衡和受限扇出，而不是套用 CMOS 风格抽象。
- 通过 PyO3 + maturin + uv 提供 Python 接口，便于脚本、Notebook 和现有流程集成。
- 逐步形成从 netlist 编译到 layout、STA、统计时序、验证与高阶约束分析的端到端流程。

更完整的背景和长期设计见 [docs/project-design.md](docs/project-design.md) 与各阶段文档 [docs/phase-0.md](docs/phase-0.md)、[docs/phase-1.md](docs/phase-1.md)、[docs/phase-2.md](docs/phase-2.md)、[docs/phase-3.md](docs/phase-3.md)、[docs/phase-4.md](docs/phase-4.md)、[docs/phase-5.md](docs/phase-5.md)、[docs/phase-6-sim.md](docs/phase-6-sim.md)。

## 当前已实现能力

基于当前代码与测试覆盖，仓库已经具备以下可执行能力。

### Rust 核心

- `rflux-ir`：基础 SFQ IR、节点/边、单消费者连接约束原型。
- `rflux-hdl`：最小可用 Rust builder DSL，可构造 port、logic cell、macro、DFF、splitter 并生成 `Netlist`。
- `rflux-synth`：
  - compile plan 批量连接
  - splitter 自动插入
  - 路径平衡 DFF 插入
  - 纯 Rust 布尔优化与兼容性分析
  - 最小技术映射与综合报告
- `rflux-place`：levelized placement、固定节点、blocked region、宏单元 halo、简单拥塞外溢。
- `rflux-route`：JTL/PTL 混合布线、boundary-aware routing、keep-out 绕障、detour 统计。
- `rflux-timing`：
  - 确定性 STA
  - pin / node / clock-domain 约束
  - false path / max delay / multicycle crossing 约束
  - 轻量级 SSTA，包括全局相关项与跨域不确定度
- `rflux-flow`：综合、布局、布线、时序、验证和 AC bias 分析的统一编排入口。
- `rflux-tech`：最小 PDK 抽象与 PTL forbidden-length 查询。
- `rflux-io`：JSON IR/PDK、LEF/DEF 基础读写路径。

### Python 接口

`python/rflux` 与 `crates/py` 已经暴露出一组可直接使用的高层 API：

- `Circuit`
- `compile_plan` / `compile_plan_report`
- `compile_netlist`
- `compile_layout`
- `analyze_timing`
- `analyze_timing_statistical`
- `verify_layout`
- `analyze_ac_bias`
- `optimize_ac_bias`
- `characterize_compound_cell`
- `analyze_advanced_constraints`

这些能力在 [python/tests/test_basic.py](python/tests/test_basic.py) 中已有端到端覆盖。

## 仓库结构

当前 workspace 的主要模块如下：

```text
rflux/
├── crates/
│   ├── flow/      # 端到端编排
│   ├── hdl/       # Rust DSL / builder
│   ├── io/        # 文件格式与交换
│   ├── ir/        # SFQ IR
│   ├── place/     # 布局原型
│   ├── py/        # PyO3 扩展
│   ├── route/     # 布线原型
│   ├── sim/       # 仿真模块骨架与外部/事件后端统一接口
│   ├── synth/     # 综合原型
│   ├── tech/      # PDK / 工艺抽象
│   └── timing/    # STA / SSTA
├── docs/          # 设计和阶段文档
├── python/rflux/  # 纯 Python facade
├── python/tests/  # Python 侧回归测试
└── src/main.rs    # 根 CLI，占位状态
```

说明：设计文档中还提到了 `device`、`verify`、`cli` 等更完整的模块拆分；其中 `sim` 现已作为独立 crate 骨架落地，但求解器能力仍在推进中，README 以下述“现有 workspace”为准。

## 环境要求

- Rust stable toolchain
- Python 3.12
- `uv`
- `maturin`

本仓库的 Python 依赖和虚拟环境统一由 `uv` 管理。不要使用 `pip install` 作为主流程。

## 安装与开发

### 1. 同步 Python 依赖

```bash
uv sync
```

### 2. 构建并安装本地 PyO3 扩展

```bash
uv run maturin develop -m crates/py/Cargo.toml
```

### 3. 运行测试

```bash
uv run cargo test --workspace
uv run pytest
```

说明：workspace 包含 PyO3 扩展 crate。在未显式配置 Python 解释器的环境中，直接运行 `cargo test --workspace` 可能会在 `pyo3-build-config` 阶段失败；使用 `uv run cargo test --workspace` 更稳妥。

如果只想快速确认 Python facade 可用，也可以运行：

```bash
uv run python -c "import rflux; print(rflux.__version__)"
```

### 4. Internal vs External waveform quick compare

Use the helper script to compare internal transient CSV traces against an external simulator run on the same deck:

```bash
uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/t_delay_smoke.cir --josim-command josim
```

The script prints per-node max-abs and RMS error metrics on shared waveform columns and a PASS/FAIL summary threshold.
It also supports `--json-output <path>` to emit structured comparison results for downstream tooling.

An optional pytest integration is also available in `python/tests/test_waveform_compare.py`.
It auto-skips when `josim` is not available on PATH and uses
`python/tests/benchmarks/phase6/waveform_thresholds.json` for per-deck max-abs thresholds.
Core waveform-compare utilities are covered by `python/tests/test_waveform_compare_utils.py`
without requiring `josim`.

To summarize JSON compare outputs into a markdown report:

```bash
uv run python python/scripts/summarize_waveform_compare_results.py --result-dir python/tests/benchmarks/phase6 --markdown-output python/tests/benchmarks/phase6/waveform_compare_summary.md
```

The command exits non-zero if any deck fails threshold checks or if result files are missing.

### 5. Optional CI waveform compare job

`ci.yml` now includes a manual `workflow_dispatch` job named `waveform-compare-optional`.
It is disabled by default and only runs when `run_external_waveform_compare=true` is selected.

Inputs:

- `run_external_waveform_compare`: enable/disable optional job
- `josim_command`: command/path passed to waveform compare test via `RFLOW_JOSIM_COMMAND`

This keeps normal push/PR CI unchanged while allowing on-demand external correlation checks on runners where JoSIM is installed.

Quick manual trigger checklist (GitHub UI):

1. Open repository `Actions` tab and select workflow `CI`.
2. Click `Run workflow`.
3. Set `run_external_waveform_compare` to `true`.
4. Set `josim_command` if JoSIM is not available as plain `josim` on the runner PATH.
5. Start the run and verify `waveform-compare-optional` job result.

## 快速上手

### Python 示例

下面的例子展示如何构造一个最小电路，并运行 layout、时序分析和 AC bias 优化。

```python
import rflux

circuit = rflux.Circuit("demo")
src_a = circuit.add_node("port", "a")
src_b = circuit.add_node("port", "b")
gate = circuit.add_node("cell", "xor0", logic_op="xor")

circuit.connect(src_a, 0, gate, 0)
circuit.connect(src_b, 0, gate, 1)

layout = rflux.compile_layout(circuit)
print(layout.placed_nodes, layout.routed_nets, layout.critical_path_delay_ps)

timing = rflux.analyze_timing(
    circuit,
    timing_constraints=[rflux.NodeTimingConstraint(node=gate, required_ps=120.0)],
)
print(timing.worst_setup_slack_ps, timing.analyzed_timing_arcs)

ac_bias = rflux.optimize_ac_bias(circuit)
print(ac_bias.optimized.optimization_score)
```

### Rust HDL builder 示例

```rust
use rflux_hdl::CircuitBuilder;
use rflux_ir::LogicOp;

let mut builder = CircuitBuilder::new();
let input = builder.port("in");
let gate = builder.logic_cell("xor0", LogicOp::Xor);
let stage = builder.dff("stage0");
let output = builder.port("out");

builder
    .connect(input, gate)?
    .connect(gate, stage)?
    .connect(stage, output)?;

let netlist = builder.finish();
assert_eq!(netlist.node_count(), 4);
```

## 当前成熟度说明

`rflux` 目前适合作为以下用途：

- SFQ EDA 研究原型
- Rust/Python 混合工具链验证
- 布局布线与时序接口联调
- Python Notebook / 脚本分析的后端核心

当前不应假设以下内容已经产品化：

- 完整 CLI 工作流
- 生产级 PDK 与标准单元库
- 大规模、精确的器件级仿真闭环
- 完整 GDS 导出与签核流程
- 稳定的公共 API 承诺

## 设计原则

- 核心 crate 优先保持纯 Rust，避免破坏可移植性和 wasm 构建链。
- Python 层做薄封装，不复制综合、STA、布线等核心逻辑。
- 以 IR 为主设计载体，Verilog 只是输入前端之一。
- 设计文档描述的是目标架构，README 描述的是当前仓库状态。

## 相关文档

- [docs/project-design.md](docs/project-design.md): 总体设计、模块规划、技术背景
- [docs/sfq.md](docs/sfq.md): SFQ 领域背景
- [docs/phase-6-sim.md](docs/phase-6-sim.md): `rflux-sim` 推进计划与阶段出口条件
- [docs/josim-parity.md](docs/josim-parity.md): `rflux-sim` 对标 JoSIM 的功能矩阵与进度基线
- [AGENTS.md](AGENTS.md): 仓库协作约定，尤其是 Python/uv 规则
- [python/tests/test_basic.py](python/tests/test_basic.py): 当前 Python API 的实际用法参考

## 许可证

本 workspace 当前使用双许可证：`MIT OR Apache-2.0`。
