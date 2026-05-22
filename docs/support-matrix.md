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
| 操作系统 | Windows | 受限支持 | 仓库可在 Windows 开发，但当前无正式 CI 矩阵覆盖。 |
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
| 预编译 wheel | 实验性 | 当前仓库未提供正式发布流程。 |
| 预编译 CLI 二进制 | 实验性 | 当前仓库未提供正式发布流程。 |

## 4. 输入支持矩阵

### 4.1 电路与网表输入

| 输入类型 | 等级 | 说明 |
|----------|------|------|
| `rflux-ir` JSON | 正式验证 | 当前最稳定、最推荐的基线输入路径。 |
| Rust `rflux-hdl` builder DSL | 受限支持 | 适合内部构造与测试；并非通用 HDL frontend。 |
| 通用 Verilog frontend | 实验性 | 当前不应对外承诺通用 Verilog 支持。 |
| LEF/DEF 交换路径 | 受限支持 | 已有基础 I/O 路径，但不等于成熟全流程交换兼容。 |

### 4.2 仿真输入

| 输入类型 | 等级 | 说明 |
|----------|------|------|
| `simulate_text(...)` 受限 SPICE/JoSIM 子集 | 受限支持 | 当前可用于已覆盖子集；不是通用 SPICE frontend。 |
| `simulate_file(...)` + 相对 `.include` | 受限支持 | 当前可用，但语义覆盖仍在扩展。 |
| 通用 SPICE deck | 实验性 | 不应宣称完整支持。 |

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
| `lint-input` | 受限支持 | 当前用于 IR / PDK JSON 预检查与 schema 兼容窗口识别。 |
| `compile-netlist` | 受限支持 | 推荐搭配 IR JSON 输入。 |
| `compile-layout` | 受限支持 | 当前输出用于原型级物理分析。 |
| `analyze-timing` | 受限支持 | 当前结果不可表述为 signoff 报告。 |
| `verify-layout` | 受限支持 | 结构与受限仿真检查路径可用。 |
| `simulate-file` | 受限支持 | 受当前 deck 语义子集约束。 |
| `solve-dimacs` | 正式验证 | 已有针对性测试与 CLI 工作流。 |
| `check-equivalence` | 受限支持 | 当前是主要对外交付验证路径之一。 |

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
| 自定义 PDK JSON 导入 | 受限支持 | 可用，但尚未建立商业级 PDK validate 与兼容策略。 |
| 多套正式发布 PDK | 实验性 | 当前尚未建立产品化流程。 |

## 9. CI 与验证覆盖

| 检查项 | 等级 | 说明 |
|--------|------|------|
| `cargo test --workspace` | 正式验证 | 默认 CI 覆盖。 |
| `uv run pytest` | 正式验证 | 默认 CI 覆盖。 |
| 外部 waveform compare | 受限支持 | 手动触发工作流，可选执行。 |
| 多平台矩阵 | 实验性 | 当前缺失。 |
| nightly fuzz / benchmark / compatibility suite | 实验性 | 当前未建立。 |

## 10. 当前不承诺事项

以下事项在当前版本中不构成正式承诺：

- 通用 Verilog / HDL 商业级导入支持。
- signoff 级 P&R、STA、SSTA 或仿真结论。
- 全 SPICE / 全 JoSIM 语义兼容。
- 多平台正式发布包与兼容矩阵。
- 多 PDK 商业化维护承诺。
- 向后兼容的长期稳定 schema 契约。

## 11. 使用建议

- 对外试用、基准和回归请优先使用 IR JSON 输入路径。
- 对关键结果请始终保留外部对照，不要把当前 prototype 报告直接当作最终签核依据。
- 对任何实验性或受限支持能力，必须同时记录输入版本、配置、PDK 与运行环境。

## 12. 维护规则

每次版本发布前，必须更新本文件中的以下内容：

- 新增正式验证平台。
- 新增或降级支持能力。
- 当前已知不承诺事项。
- 与发布版本绑定的 CI 结果与验证说明。