# rflux 支持矩阵

## 1. 目的

本文件定义 `rflux` 当前版本的支持边界、验证等级和使用前提。

支持等级分为三类：

- `正式验证`：进入默认 CI 或有明确回归验证，团队可据此排查问题。
- `受限支持`：有代码路径和局部测试，但缺少完整 CI、兼容性或规模验证。
- `实验性`：仅用于研发、对齐或原型验证，不可视为稳定承诺。

当前版本号基线：`0.1.0`

## 2. 平台支持

| 维度 | 范围 | 等级 | 说明 |
|------|------|------|------|
| 操作系统 | Ubuntu latest | 正式验证 | 当前 GitHub Actions 默认检查运行在 Ubuntu。 |
| 操作系统 | Windows latest | 正式验证 | 当前已新增 `core-smoke-windows` job 覆盖 CLI 最小链路与 Python 最小链路。 |
| 操作系统 | macOS | 实验性 | 当前无自动化验证。 |
| Rust toolchain | stable | 正式验证 | README 与 CI 均以 stable 为基线。 |
| Python | 3.12 | 正式验证 | `.python-version` 与 `pyproject.toml` 已锁定 3.12。 |
| Python | 3.13+ | 实验性 | 当前未声明兼容，也无自动化验证。 |
| 包管理 | `uv` | 正式验证 | Python 依赖与虚拟环境的唯一标准路径。 |
| PyO3 构建 | `maturin` 1.6+ | 正式验证 | 当前 Python 扩展构建基线。 |

## 3. 分发与安装支持

| 交付面 | 等级 | 当前状态 |
|--------|------|----------|
| 源码构建 | 正式验证 | `uv sync` + `uv run maturin develop -m crates/py/Cargo.toml` |
| Rust CLI 本地运行 | 正式验证 | `cargo run -p rflux-cli -- ...` |
| Python 本地导入 | 正式验证 | `uv run python -c "import rflux"` |
| 预编译 wheel | 实验性 | 当前仍未提供正式对外发布流程，但已可通过手动 `release-artifacts-optional` job 生成当前 runner 的候选 wheel bundle。 |
| 预编译 CLI 二进制 | 实验性 | 当前仍未提供正式对外发布流程，但已可通过手动 `release-artifacts-optional` job 生成当前 runner 的候选 CLI 二进制 bundle。 |

## 4. 输入支持矩阵

### 4.0 首批对外输入边界

当前版本面向外部试用时，输入路径应按以下三档理解：

| 档位 | 当前路径 | 对外口径 |
|------|----------|----------|
| 首批正式基线 | `rflux-ir` JSON | 默认推荐；问题排查、基准回归和支持受理应优先要求该路径复现。 |
| 首批受限支持 | 受限 SPICE / JoSIM deck、Rust `rflux-hdl` builder DSL、自定义 PDK JSON | 可用，但必须结合已知限制、样例回归和版本信息一起使用。 |
| 当前不承诺 | 通用 Verilog frontend、通用 SPICE deck | 不应作为首批商业化或外部试用承诺入口。 |

补充规则：

- 任何新输入路径在没有进入本文件前，一律不得默认视为正式支持。
- 任何受限支持输入在提交缺陷时，应同时附带版本、配置、PDK 和最小复现样例。
- 对外文档、示例、benchmark 和支持工单，默认都应优先使用 `rflux-ir` JSON 作为复现基线。

### 4.1 电路与网表输入

| 输入类型 | 等级 | 说明 |
|----------|------|------|
| `rflux-ir` JSON | 正式验证 | 当前最稳定、最推荐的基线输入路径。 |
| 最小 `.bench` 组合逻辑子集 | 受限支持 | 当前 CLI 已可直接读取 Quaigh-style gate-level `INPUT`/`OUTPUT`/`AND`/`OR`/`XOR`/`XNOR`/`NOT`/`NAND`/`NOR`/`BUF`/`BUFF`/`MUX`/`MAJ`/`AOI21`/`OAI21`/`AOI22`/`OAI22`/`AOI31`/`OAI31`/`AOI211`/`OAI211`/`AOI311`/`OAI311`/`AOI321`/`OAI321`/`AOI221`/`OAI221`/`AOI222`/`OAI222`/`AOI322`/`OAI322`/`AOI421`/`OAI421`/`AOI422`/`OAI422`/`AOI431`/`OAI431`/`AOI432`/`OAI432`/`AOI433`/`OAI433`/`AOI441`/`OAI441`/`AOI442`/`OAI442`/`AOI443`/`OAI443`/`AOI444`/`OAI444`/`AOI2221`/`OAI2221` 子集，其中 `XNOR`/`NAND`/`NOR`/`MAJ`/`AOI21`/`OAI21`/`AOI22`/`OAI22`/`AOI31`/`OAI31`/`AOI211`/`OAI211`/`AOI311`/`OAI311`/`AOI321`/`OAI321`/`AOI221`/`OAI221`/`AOI222`/`OAI222`/`AOI322`/`OAI322`/`AOI421`/`OAI421`/`AOI422`/`OAI422`/`AOI431`/`OAI431`/`AOI432`/`OAI432`/`AOI433`/`OAI433`/`AOI441`/`OAI441`/`AOI442`/`OAI442`/`AOI443`/`OAI443`/`AOI444`/`OAI444`/`AOI2221`/`OAI2221` 通过前端 lowering 进入现有 IR；但这仍不应表述为通用 HDL frontend。 |
| Rust `rflux-hdl` builder DSL | 受限支持 | 适合内部构造与测试；并非通用 HDL frontend。 |
| 通用 Verilog frontend | 实验性 | 当前不应对外承诺通用 Verilog 支持。 |
| LEF/DEF 交换路径 | 受限支持 | 已有基础 I/O 路径，但不等于成熟全流程交换兼容。 |

### 4.2 仿真输入

| 输入类型 | 等级 | 说明 |
|----------|------|------|
| `simulate_text(...)` 受限 SPICE/JoSIM 子集 | 受限支持 | 当前可用于已覆盖子集；不是通用 SPICE frontend。 |
| `simulate_file(...)` + 相对 `.include` | 受限支持 | 当前可用，但语义覆盖仍在扩展。 |
| 通用 SPICE deck | 实验性 | 不应宣称完整支持。 |

### 4.3 当前默认复现入口

为减少支持与验证歧义，当前默认复现入口按以下顺序处理：

1. IR / PDK / CLI JSON 问题：优先要求版本化 JSON 输入或 `lint-input` 输出。
2. flow / verify / timing 问题：优先要求 `rflux-ir` JSON + PDK JSON + CLI 命令行。
3. sim 问题：优先要求原始 deck、明确的 `simulation_mode`，以及外部 JoSIM 对照结果（若可获得）。

若用户无法提供上述最小复现材料，问题仍可受理，但不应视为正式支持路径下的完整缺陷确认。

## 5. 核心能力支持矩阵

| 能力 | 等级 | 说明 |
|------|------|------|
| IR 建模与 JSON round-trip | 正式验证 | 作为全仓库核心数据通路。 |
| SFQ 综合（splitter、path balancing、最小 tech mapping） | 受限支持 | 已具备稳定实现，但能力边界仍偏 prototype。 |
| 物理实现（place/route） | 受限支持 | 当前为可执行原型，不是 signoff 级 P&R。 |
| 确定性 STA | 受限支持 | 可用，但模型与约束语义仍需更强对照验证。 |
| 轻量 SSTA | 实验性 | 用于研究和探索，不可视为签核结论。 |
| 组合等价检查 | 受限支持 | 当前是较强能力之一，但仍需更正式的兼容与规模说明。 |
| 单步时序等价检查 | 受限支持 | 目前聚焦 `Dff` / `DffEnable` 子集。 |
| 内部瞬态仿真 | 受限支持 | 当前为受限器件子集，不是 JoSIM 级完整仿真。 |
| 外部 JoSIM 驱动 | 受限支持 | 依赖外部工具存在，结果契约需继续稳定化。 |
| waveform compare 辅助脚本 | 受限支持 | 已进入可选测试链路，但不是默认发布门。 |

## 6. CLI 支持矩阵

| 命令 | 等级 | 说明 |
|------|------|------|
| `pdk-minimal` | 正式验证 | 用于最小 PDK 生成与 smoke。 |
| `pdk-validate` | 受限支持 | 当前提供最小 PDK 结构校验、cell library index 摘要、分项 checks 与 advisory warnings，用于自定义 PDK 接入前的第一道门槛。 |
| `pdk-cell-library` | 受限支持 | 输出 PDK cell library 机器可读索引，可按 cell 名称或 kind 过滤，用于接入脚本和评审检查。 |
| `lint-input` | 受限支持 | 当前用于 IR / PDK JSON 预检查与 schema 兼容窗口识别。 |
| `compile-netlist` | 受限支持 | 推荐搭配 IR JSON 输入；当前也接受最小 `.bench` 组合逻辑子集。 |
| `compile-layout` | 受限支持 | 当前输出用于原型级物理分析。 |
| `analyze-timing` | 受限支持 | 当前结果不可表述为 signoff 报告。 |
| `verify-layout` | 受限支持 | 结构与受限仿真检查路径可用。 |
| `simulate-file` | 受限支持 | 受当前 deck 语义子集约束。 |
| `solve-dimacs` | 正式验证 | 已有针对性测试与 CLI 工作流。 |
| `check-equivalence` | 受限支持 | 当前是主要对外交付验证路径之一。 |

当前 `solve-dimacs` 的正式验证状态对应的显式 CI 锚点是：`cargo test -p rflux-sat --test dimacs_end_to_end -- --nocapture`。

## 7. Python API 支持矩阵

| API 面 | 等级 | 说明 |
|--------|------|------|
| `Circuit` 基础建模 | 正式验证 | 当前 Python facade 基础入口。 |
| `compile_plan` / `compile_netlist` / `compile_layout` | 受限支持 | 可用，但缺少已编译 `rflux._core` 扩展时会显式失败。 |
| `analyze_timing` / `analyze_timing_statistical` | 受限支持 / 实验性 | 两条路径当前都要求已编译 `rflux._core` 扩展；统计路径仍属研究导向。 |
| `verify_layout` | 受限支持 | 当前要求已编译 `rflux._core` 扩展，不再提供 Python fallback 验证结果。 |
| `simulate_text` / `simulate_file` | 受限支持 | 仅读作当前 parser / solver 子集承诺。 |
| `compile(...)` | 实验性 | 当前会显式抛出 `NotImplementedError`，用于避免把占位接口误用为正式编译入口。 |

## 8. PDK 与工艺支持

| 项目 | 等级 | 说明 |
|------|------|------|
| `Pdk::minimal("minimal-sfq")` | 正式验证 | 当前仓库最稳定的基线 PDK。 |
| 自定义 PDK JSON 导入 | 受限支持 | 可用，且已有最小 `pdk-validate` 入口；但仍未建立商业级 validate、迁移与兼容策略。 |
| 多套正式发布 PDK | 实验性 | 当前尚未建立产品化流程。 |

当前使用前提：

- 对外复现与回归默认应优先使用 `minimal-sfq` 或仓库内已明确标注版本的 PDK JSON。
- 自定义 PDK 在进入正式评估前，至少应同时提供版本信息、来源说明、最小样例和一次独立验证记录。
- 当前最小 `pdk-validate` 能提供结构级校验、覆盖摘要、分项检查和 characterization 相关 advisory warnings，但仍不替代 benchmark、flow 回归和人工建模审查。

## 9. 外部依赖支持矩阵

| 依赖项 | 等级 | 说明 |
|--------|------|------|
| Rust stable toolchain | 正式验证 | 当前 CLI 和 workspace 默认基线。 |
| `uv` | 正式验证 | 当前唯一受支持的 Python 环境与依赖管理路径。 |
| `maturin` 1.6+ | 正式验证 | Python 扩展本地构建基线。 |
| 外部 `josim` / `josim-cli` 可执行文件及受限 wrapper 后缀 | 受限支持 | 仅用于当前 external simulator 路径；允许 `.exe` / `.cmd` / `.bat` / `.sh` 后缀和路径形式，但仍按最终文件名做 allowlist 匹配。结果契约和安装方式仍需继续稳定化。 |
| 其他外部仿真器或自定义命令 | 实验性 | 当前不构成正式支持路径。 |

当前使用前提：

- 外部依赖问题受理时，应同时提供工具版本、安装来源、命令行和运行平台。
- `external_josim` 相关问题默认需要可复现的 deck、外部可执行文件版本，以及与内部模式的差异说明。
- 未进入本表的外部程序、脚本或系统库依赖，不应被表述为当前正式支持的一部分。

## 10. CI 与验证覆盖

| 检查项 | 等级 | 说明 |
|--------|------|------|
| `cargo test --workspace` | 正式验证 | 默认 CI 覆盖。 |
| `uv run pytest` | 正式验证 | 默认 CI 覆盖。 |
| Windows 核心 smoke（CLI） | 正式验证 | `core-smoke-windows` 直接锚定 `lint-input / compile-netlist / check-equivalence` 三条命令链路。 |
| Windows 核心 smoke（Python） | 正式验证 | `core-smoke-windows` 运行 `python/tests/test_basic.py` 的最小子集（包版本、结构化 API 重导出、simulate_file 路径）。 |
| 外部 waveform compare | 受限支持 | 手动触发工作流，可选执行。 |
| 多平台矩阵 | 受限支持 | 当前具备 Ubuntu 默认检查 + Windows 核心 smoke；尚未扩展到 macOS 或完整对称矩阵。 |
| nightly fuzz / benchmark / compatibility suite | 实验性 | 当前未建立。 |

## 11. 当前不承诺事项

以下事项在当前版本中不构成正式承诺：

- 通用 Verilog / HDL 商业级导入支持。
- signoff 级 P&R、STA、SSTA 或仿真结论。
- 全 SPICE / 全 JoSIM 语义兼容。
- 多平台正式发布包与兼容矩阵。
- 多 PDK 商业化维护承诺。
- 未列入支持矩阵的外部依赖或安装方式。
- 向后兼容的长期稳定 schema 契约。

## 12. 使用建议

- 对外试用、基准和回归请优先使用 IR JSON 输入路径；`.bench` 仅适合作为当前受限前端对齐入口。
- 对关键结果请始终保留外部对照，不要把当前 prototype 报告直接当作最终签核依据。
- 对任何实验性或受限支持能力，必须同时记录输入版本、配置、PDK 与运行环境。

## 13. 维护规则

每次版本发布前，必须更新本文件中的以下内容：

- 新增正式验证平台。
- 新增或降级支持能力。
- 新增、移除或降级外部依赖支持项。
- 当前已知不承诺事项。
- 与发布版本绑定的 CI 结果与验证说明。
