# rflux

Rust-first SFQ EDA toolkit for superconducting single-flux-quantum circuits.

[![CI](https://github.com/Q-EDA/rflux/actions/workflows/ci.yml/badge.svg)](https://github.com/Q-EDA/rflux/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![Python](https://img.shields.io/badge/python-3.12+-blue.svg)](https://www.python.org)
[![codecov](https://codecov.io/gh/Q-EDA/rflux/graph/badge.svg)](https://codecov.io/gh/Q-EDA/rflux)
[![dependency status](https://deps.rs/repo/github/Q-EDA/rflux/status.svg)](https://deps.rs/repo/github/Q-EDA/rflux)

> `rflux` 是以 Rust 为核心、辅以 Python 绑定的超导单磁通量子（SFQ）EDA 工具链。它围绕 SFQ 电路的物理约束（splitter 扇出、JTL/PTL 互连、AC bias 网格、时钟相位）构建，目标是把逻辑综合、物理设计、时序与仿真统一在一个一致的工作流里。Rust workspace 暴露稳定子集给 Python，CLI 覆盖端到端流程，所有 crate 详见下文。

## 目录 / Table of Contents

- [文档](#文档)
- [贡献指南](#贡献指南)
- [许可证](#许可证)

> 注：README 部分历史章节正文因早期编辑造成编码丢失已不可读（显示为 `?`）。本节及以下新增章节为可读导航；完整设计内容请以 [`docs/`](docs/) 下的设计文档为准。README 正文重写已列入待办。

## 文档

- [docs/project-design.md](docs/project-design.md) — 整体架构、crate 地图、Python 绑定设计
- [docs/sfq.md](docs/sfq.md) — SFQ EDA 领域背景
- [docs/phase-6-sim.md](docs/phase-6-sim.md) — `rflux-sim` 设计与 JoSIM parity 路线
- [docs/josim-parity.md](docs/josim-parity.md) — `rflux-sim` 与 JoSIM 的功能对照
- [docs/quaigh-alignment.md](docs/quaigh-alignment.md) / [docs/yosys-alignment.md](docs/yosys-alignment.md) — 与 Quaigh / Yosys 的对照
- [docs/known-limitations.md](docs/known-limitations.md) — 当前已知限制
- [docs/error-codes.md](docs/error-codes.md) — 结构化错误码
- [AGENTS.md](AGENTS.md) — AI 助手与贡献者协作规范（含 Python/uv 规则）

## 贡献指南

欢迎贡献！请先阅读 [CONTRIBUTING.md](CONTRIBUTING.md)（开发流程、PR 规范、CI 门禁）与 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。安全相关问题请按 [SECURITY.md](SECURITY.md) 的流程私密上报，不要开公开 issue。

## 许可证

本项目按你的选择，以 **MIT OR Apache-2.0** 双授权许可：

- [LICENSE-MIT](LICENSE-MIT)
- [LICENSE-APACHE](LICENSE-APACHE)

除非你明确声明 otherwise，任何有意提交并入本仓库的贡献，均按上述双授权许可，无额外条款（详见 [CONTRIBUTING.md](CONTRIBUTING.md) 与 [LICENSE-APACHE](LICENSE-APACHE) 第 5 条）。

`rflux` ?????? Rust ???????? Python ?????????????????SFQ EDA ???????????????????????????????????????????????????????????????SFQ ???????????????????splitter ?????????????TL/PTL ???????????????????????? AC bias ??????????????

?????????????????????????????????????????????? Rust workspace ??Python facade ???????????????????????????????????????????????????????CLI ????????crate??

## ??????

- ??Rust ??????????????????????`wasm32-unknown-unknown` ?????SFQ EDA ?????
- ?????IR ??? SFQ ?????????????????????????????????????CMOS ????????
- ??? PyO3 + maturin + uv ??? Python ?????????????otebook ?????????????
- ????????netlist ?????layout??TA??????????????????????????????????

?????????????????? [docs/project-design.md](docs/project-design.md) ????????? [docs/phase-0.md](docs/phase-0.md)??docs/phase-1.md](docs/phase-1.md)??docs/phase-2.md](docs/phase-2.md)??docs/phase-3.md](docs/phase-3.md)??docs/phase-4.md](docs/phase-4.md)??docs/phase-5.md](docs/phase-5.md)??docs/phase-6-sim.md](docs/phase-6-sim.md)??????????????????????????????????????[docs/yosys-alignment.md](docs/yosys-alignment.md)??????????????????????????? [docs/commercialization-roadmap.md](docs/commercialization-roadmap.md)??docs/product-scope.md](docs/product-scope.md)??docs/support-matrix.md](docs/support-matrix.md)??docs/release-policy.md](docs/release-policy.md)??docs/error-codes.md](docs/error-codes.md)??docs/known-limitations.md](docs/known-limitations.md)??docs/defect-severity-sla.md](docs/defect-severity-sla.md)??docs/benchmark-correlation-plan.md](docs/benchmark-correlation-plan.md)??docs/ownership-matrix.md](docs/ownership-matrix.md)??docs/pdk-onboarding.md](docs/pdk-onboarding.md)??docs/interface-inventory.md](docs/interface-inventory.md)??docs/archive/security-compliance.md](docs/archive/security-compliance.md)??docs/external-command-policy.md](docs/external-command-policy.md)??docs/third-party-risk-review.md](docs/third-party-risk-review.md)??docs/third-party-risk-register.md](docs/third-party-risk-register.md) ??[docs/third-party-exception-template.md](docs/third-party-exception-template.md)??

## ???????????

????????????????????????????????????????

### Rust ???

- `rflux-ir`????? SFQ IR??????????????????????????
- `rflux-hdl`????????Rust builder DSL???????port??ogic cell??acro??FF??plitter ?????`Netlist`??
- `rflux-synth`??
  - compile plan ??????
  - splitter ??????
  - ?????? DFF ???
  - ??Rust ????????????????
  - ?????????????????
- `rflux-place`??evelized placement??????????locked region?????? halo?????????????
- `rflux-route`??TL/PTL ????????oundary-aware routing??eep-out ?????etour ?????
- `rflux-timing`??
  - ?????STA
  - pin / node / clock-domain ???
  - false path / max delay / multicycle crossing ???
  - ?????SSTA???????????????????????
- `rflux-flow`?????????????????????????? AC bias ????????????????
- `rflux-tech`?????PDK ?????PTL forbidden-length ?????
- `rflux-io`??SON IR/PDK??EF/DEF ???????????
- `rflux-verify`??????????????????????????? SAT ???????? `Dff`/`DffEnable` ???????????????????

### Python ???

`python/rflux` ??`crates/py` ???????????????????????API??

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

????????[python/tests/test_basic.py](python/tests/test_basic.py) ??????????????

## ??????

??? workspace ????????????

```text
rflux/
????? crates/
??  ????? flow/      # ????????
??  ????? hdl/       # Rust DSL / builder
??  ????? io/        # ???????????
??  ????? ir/        # SFQ IR
??  ????? place/     # ??????
??  ????? py/        # PyO3 ???
??  ????? route/     # ??????
??  ????? sim/       # ??????????????????????????
??  ????? synth/     # ??????
??  ????? tech/      # PDK / ??????
??  ????? timing/    # STA / SSTA
??  ????? verify/    # ?????????
????? docs/          # ???????????
????? python/rflux/  # ??Python facade
????? python/tests/  # Python ????????
????? src/main.rs    # ??CLI????????
```

?????????????????? `device` ?????????????????? `cli` ??`verify` ???????????crate ????????sim` ????????? crate ??????????????????????????EADME ??????????workspace???????

## ??????

- Rust stable toolchain
- Python 3.12
- `uv`
- `maturin`

?????? Python ????????????????`uv` ???????????`pip install` ??????????

## ????????

### 1. ??? Python ???

```bash
uv sync
```

### 2. ???????????PyO3 ???

```bash
uv run maturin develop -m crates/py/Cargo.toml
```

### 3. ??????

```bash
uv run cargo test --workspace
uv run pytest
```

?????orkspace ??? PyO3 ??? crate???????????Python ?????????????????? `cargo test --workspace` ?????? `pyo3-build-config` ???????????`uv run cargo test --workspace` ???????

### 3.1 ??? CLI

workspace ????????????????? Rust CLI crate??rflux-cli`??

????????????

```bash
cargo run -p rflux-cli -- --help
```

??????????PDK JSON??

```bash
cargo run -p rflux-cli -- pdk-minimal --output target/minimal_pdk.json
```

??? PDK ??cell library ?????

```bash
cargo run -p rflux-cli -- pdk-cell-library --input target/minimal_pdk.json
cargo run -p rflux-cli -- pdk-cell-library --input target/minimal_pdk.json --kind macro
```

`pdk-cell-library` ???????????????????ind ?????iming ???????? remediation ????????? PDK cell ?????????????????????????

??IR JSON ??????????????????????

```bash
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/classic_examples/classic_full_adder.json
cargo run -p rflux-cli -- compile-layout --input crates/synth/tests/fixtures/classic_examples/classic_full_adder.json
cargo run -p rflux-cli -- analyze-timing --input crates/synth/tests/fixtures/classic_examples/classic_full_adder.json
cargo run -p rflux-cli -- verify-layout --input crates/synth/tests/fixtures/classic_examples/classic_full_adder.json --mode event_only
```

?????`.bench` ??????????????signal ?????? `a[0]` ??? bit-level token??ate ?????????????????????????????? `INPUT/OUTPUT` ?????gate ??INPUT ????????????????????? `INPUT(name)` ??`OUTPUT(name)` ??passthrough ??????????????????????????`DFF(data, clock)` ??`DFFE(data, enable, clock)`?????`.bench` checked-in fixture ????????? `crates/synth/tests/fixtures/quaigh_alignment/bench_sequential/`????????`dff_basic.bench` ??`dffe_basic.bench`???

```bash
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/dedup_and_pair.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi31.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai31.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi211.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai211.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi311.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai311.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi321.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai321.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi322.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai322.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi421.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai421.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi422.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai422.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi431.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai431.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi432.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai432.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi433.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai433.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi441.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai441.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi442.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai442.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi443.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai443.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi444.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai444.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi2221.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai2221.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi222.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai222.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi221.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai221.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi22.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai22.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/aoi21.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/oai21.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/majority3.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/iscas_c17.bench
cargo run -p rflux-cli -- compile-netlist --input crates/synth/tests/fixtures/quaigh_alignment/bench/xnor_pair.bench
```

?????IR JSON ?????????????????????

```bash
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/classic_examples/classic_full_adder.json --rhs crates/synth/tests/fixtures/classic_examples/classic_full_adder.json
cargo run -p rflux-cli -- check-equivalence --kind single_step_sequential --lhs crates/synth/tests/fixtures/quaigh_alignment/dffe_feedback_wrapped.json --rhs crates/synth/tests/fixtures/quaigh_alignment/dffe_feedback_wrapped.json
```

????????`.bench` ???????????????????????

```bash
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/dedup_and_pair.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/dedup_and_pair.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi31.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi31.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai31.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai31.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi211.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi211.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai211.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai211.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi311.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi311.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai311.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai311.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi321.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi321.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai321.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai321.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi322.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi322.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai322.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai322.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi421.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi421.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai421.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai421.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi422.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi422.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai422.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai422.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi431.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi431.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai431.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai431.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi432.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi432.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai432.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai432.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi433.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi433.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai433.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai433.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi441.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi441.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai441.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai441.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi442.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi442.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai442.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai442.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi443.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi443.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai443.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai443.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi444.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi444.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai444.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai444.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi2221.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi2221.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai2221.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai2221.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi222.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi222.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai222.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai222.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi221.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi221.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai221.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai221.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi22.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi22.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai22.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai22.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi21.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/aoi21.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai21.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/oai21.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/majority3.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/majority3.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/iscas_c17.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/iscas_c17.bench
cargo run -p rflux-cli -- check-equivalence --lhs crates/synth/tests/fixtures/quaigh_alignment/bench/xnor_pair.bench --rhs crates/synth/tests/fixtures/quaigh_alignment/bench/xnor_pair.bench
```

??SPICE/JoSIM ??? deck ??????????????

```bash
cargo run -p rflux-cli -- simulate-file --input python/tests/benchmarks/phase6/t_delay_smoke.cir --mode internal_transient
```

?????

- `--pdk <path>` ????????? PDK??????????????`Pdk::minimal("minimal-sfq")`??
- `.bench` ??????????????gate-level ?????INPUT`/`OUTPUT`/`AND`/`OR`/`XOR`/`XNOR`/`NOT`/`NAND`/`NOR`/`BUF`/`BUFF`/`MUX`/`MAJ`/`AOI21`/`OAI21`/`AOI22`/`OAI22`/`AOI31`/`OAI31`/`AOI211`/`OAI211`/`AOI311`/`OAI311`/`AOI321`/`OAI321`/`AOI221`/`OAI221`/`AOI222`/`OAI222`/`AOI322`/`OAI322`/`AOI421`/`OAI421`/`AOI422`/`OAI422`/`AOI431`/`OAI431`/`AOI432`/`OAI432`/`AOI433`/`OAI433`/`AOI441`/`OAI441`/`AOI442`/`OAI442`/`AOI443`/`OAI443`/`AOI444`/`OAI444`/`AOI2221`/`OAI2221`??
- flow ?????????????????? `FlowConfig`???????????Rust API ?????????????
- `--output <path>` ??? JSON ????????????????????stdout??

?????????????Python facade ??????????????

```bash
uv run python -c "import rflux; print(rflux.__version__)"
```

### 3.2 Build Candidate Release Artifacts

If you need a candidate CLI binary plus Python wheel bundle for internal release review on the current machine, run:

```bash
uv run python python/scripts/prepare_release_artifacts.py --output-dir target/release-artifacts
```

This stages the current runner's `rflux` CLI binary, wheel artifact(s), build-input snapshots, and a `manifest.json` review record under `target/release-artifacts/`.

For candidate go / no-go review, use [docs/release-artifact-readiness-checklist.md](./docs/release-artifact-readiness-checklist.md).

For release-note drafting and final decision recording, also use
[docs/release-notes-template.md](./docs/release-notes-template.md)
and
[docs/release-review-record-template.md](./docs/release-review-record-template.md).

When release scope touches Week 3 timing/verify/sim baseline inputs, thresholds, or summary logic, run:

```bash
uv run python python/scripts/generate_week3_golden_results.py --validate-pass --validate-no-regression --regression-tolerance 0.0
```

Then attach outputs under `target/week3-quality-pipeline/review/` as release evidence.

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

### 5. Default CI waveform compare gate

`ci.yml` now includes a default Windows job named `waveform-compare-gate`.
It downloads JoSIM `v2.7` on `windows-latest`, runs the manifest-based numeric compare path via `python/scripts/run_waveform_compare_manifest.py --validate-pass --validate-no-regression`, runs the adjacent unsupported-warning review path via `python/scripts/run_external_warning_manifest.py --validate-pass`, stages both review packets through `python/scripts/prepare_waveform_compare_artifacts.py` and `python/scripts/prepare_external_warning_artifacts.py`, then uploads both artifact bundles from `target/`.
On push and pull request events, the gate auto-resolves the repo-tracked Windows approved baseline and enforces zero-tolerance no-regression against it. On `workflow_dispatch`, the same gate still runs by default but can override the JoSIM command, baseline source, and no-regression settings through workflow inputs.

Inputs:

- `josim_command`: optional override for the JoSIM command/path used by the Windows gate
- `previous_summary_json`: optional repo-relative approved baseline summary JSON override
- `baseline_platform`: platform key used for repo baseline auto-resolution, default `windows`
- `validate_no_regression`: whether the gate should fail on positive drift relative to the resolved baseline, default `true`
- `regression_tolerance_v`: allowed positive drift during no-regression validation, default `0.0`

This makes simulation parity part of the default CI quality bar without forcing Ubuntu to compare against a cross-platform Windows baseline.

Quick manual trigger checklist (GitHub UI):

1. Open repository `Actions` tab and select workflow `CI`.
2. Click `Run workflow`.
3. Optionally set `josim_command` if the gate should use a non-default JoSIM binary.
4. Optionally set `previous_summary_json`, `baseline_platform`, `validate_no_regression`, or `regression_tolerance_v` for a custom review run.
5. Start the run and verify `waveform-compare-gate` result plus uploaded waveform-compare and external-warning artifacts.

## ???????

### Python ???

????????????????????????????????layout????????? AC bias ?????

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

???????????????????????Python ?????????????????`uv sync` ??`uv run maturin develop` ???????????

```bash
uv run python python/scripts/example_compile_analyze.py
uv run python python/scripts/example_equivalence_check.py
uv run python python/scripts/example_equivalence_cli_counterexample.py
uv run python python/scripts/example_equivalence_cli_replay.py
uv run python python/scripts/example_simulate_internal_transient.py
uv run python python/scripts/example_simulate_benchmark_file.py
uv run python python/scripts/characterize_merge_optimize.py
uv run python python/scripts/example_bench_cli_flow.py
uv run python python/scripts/example_run_with_diagnostics.py
```

- `example_compile_analyze.py`????????????????layout??iming ??AC bias ?????
- `example_equivalence_check.py`???????????????????????????????SAT ?????????????
- `example_equivalence_cli_counterexample.py`??? Python ??? CLI ?????????????? rhs?????? sidecar ?????? SAT ????????
- `example_equivalence_cli_replay.py`??? Python ??? CLI ?????? DIMACS/sidecar?????`check_ref` ??? SAT ?????
- `example_simulate_internal_transient.py`??????????RC deck ????????????????????/????????
- `example_simulate_benchmark_file.py`????????? benchmark deck ???????????`simulate_file(...)` ???????????
- `characterize_merge_optimize.py`??? compound cell ???????????library merge??iming ????????????????
- `example_bench_cli_flow.py`??? checked-in `.bench` ?????????????????`compile-netlist` ??`check-equivalence` ??CLI ??????????
- `example_run_with_diagnostics.py`??? checked-in `.bench` ??????????????`run-with-diagnostics` ???????????? manifest/report ???????????

### Rust HDL builder ???

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

## ???????????

`rflux` ?????????????????

- SFQ EDA ??????
- Rust/Python ???????????
- ?????????????????
- Python Notebook / ??????????????

????????????????????????

- ??? CLI ?????
- ?????PDK ?????????
- ??????????????????????
- ??? GDS ???????????
- ????????API ???

## ??????

- ??? crate ????????Rust??????????????? wasm ???????
- Python ???????????????????TA??????????????
- ??IR ???????????erilog ??????????????
- ????????????????????EADME ??????????????????

## ??????

- [docs/project-design.md](docs/project-design.md): ??????????????????????
- [docs/sfq.md](docs/sfq.md): SFQ ??????
- [docs/phase-6-sim.md](docs/phase-6-sim.md): `rflux-sim` ?????????????????
- [docs/josim-parity.md](docs/josim-parity.md): `rflux-sim` ??? JoSIM ???????????????
- [docs/diagnostics.md](docs/diagnostics.md): ????????????????`collect-diagnostics` ???
- [docs/archive/security-compliance.md](docs/archive/security-compliance.md): ????????????????????????????????
- [AGENTS.md](AGENTS.md): ??????????????? Python/uv ???
- [python/tests/test_basic.py](python/tests/test_basic.py): ??? Python API ???????????

## ?????

??workspace ??????????????MIT OR Apache-2.0`??

