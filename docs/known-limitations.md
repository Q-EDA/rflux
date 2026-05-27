# rflux 当前已知限制

## 1. 目的

本文件列出当前版本 `rflux` 的主要已知限制。它的目标不是降低预期，而是防止用户把 prototype 能力误用为商业级承诺。

所有限制项都应遵循以下规则：

- 限制必须明确。
- 限制必须可定位到模块或能力边界。
- 限制必须说明推荐替代路径。

## 2. 总体定位限制

当前 `rflux` 仍然是研究型原型向商业化过渡中的系统，而不是 signoff 级工业平台。

因此当前版本不应被用于：

- 作为唯一签核依据。
- 作为通用 HDL / SPICE 商业编译器替代品。
- 作为多平台正式发布产品直接交付给无共研背景用户。

## 3. 输入与前端限制

### 3.1 HDL 前端

- 当前仓库没有成熟的通用 Verilog frontend 承诺。
- `rflux-hdl` 更接近内部 Rust builder DSL，而不是商业级 HDL 导入层。

建议：

- 当前优先使用 IR JSON 作为正式输入路径。

### 3.2 SPICE / JoSIM deck

- 当前 `simulate_text(...)` / `simulate_file(...)` 只支持受限语法子集。
- 不支持的语法不应被视为 bug，除非该语法已进入支持矩阵。
- deck 兼容范围仍在持续推进，尚未达到通用 SPICE 兼容水平。

建议：

- 对关键 deck 保留外部 JoSIM 对照。
- 对超出当前子集的 deck 使用 `external_josim` 模式。

## 4. 综合与验证限制

- 综合路径当前聚焦 SFQ 特定约束，例如 splitter 与 path balancing。
- 技术映射仍然偏最小子集，不应宣称覆盖成熟商用标准单元映射生态。
- 单步时序等价检查当前仅覆盖受限 sequential 子集。

建议：

- 对复杂时序逻辑保持额外等价与样例回放验证。

## 5. 物理实现限制

- `place` / `route` 当前是可执行物理原型，不是 signoff 级 P&R。
- 当前拥塞建模、CTS、闭环优化和大规模 obstacle-aware routing 仍不完整。
- 当前布局布线结果更适合用于早期架构与流程验证，而不是最终交付版图签核。

建议：

- 将现有 physical 结果用于 early-stage QoR 评估和算法验证。
- 对最终物理结论保留人工审查与外部对照。

## 6. 时序分析限制

- 当前确定性 STA 已可运行，但仍缺更强的外部对照和签核语义定义。
- 当前 SSTA 属于研究和探索导向，不能作为正式签核依据。
- waveform-aware SFQ timing 语义尚不完整。

建议：

- 对关键路径报告保留手工或外部对照。

## 7. 仿真限制

- 当前内部 transient 只覆盖受限器件与语法子集。
- 当前与 JoSIM 的对齐仍是 partial，而非完整语义兼容；虽然 native `.model ... jj(... cpr={...})` 与实例侧 `J... cpr={...}` 的五系数子集现已进入支持面，但更高阶 CPR 和更完整模型语义仍未完成。
- 当前噪声、JJ、传输线等支持路径虽然已有进展，但仍不应表述为 JoSIM 级完整能力。
- 当前外部仿真调用已收口为最小 allowlist 和最小路径信任规则，仅接受 `josim` / `josim-cli` 及其受限 wrapper 后缀（`.exe` / `.cmd` / `.bat` / `.sh`）；尚未提供更通用的外部仿真器配置策略。
- 当前已具备默认启用的 Windows manifest-based JoSIM 数值对齐 CI 路径（`waveform-compare-gate`）；现有 phase-6 基准中的 pure second-harmonic JJ、pure third-harmonic JJ、pure fourth-harmonic JJ 与 pure fifth-harmonic JJ 都已进入数值对齐资产。warning-contract review bundle 仍保留为流程能力，但当前基准合同可以为空。Ubuntu runner 仍缺 same-platform Linux approved baseline，因此严格 no-regression gate 还不能在 Linux 默认开启。

建议：

- 对关键电路保留 external simulator correlation。
- 将 internal transient 视为受限、快速、可测试的原生路径，而不是最终精确替代。

## 8. PDK 与工艺限制

- 当前最稳定路径仍依赖 `minimal-sfq` 基线 PDK。
- 自定义 PDK 导入尚未完成商业级 validate、schema 迁移和版本策略。
- 尚未建立多套 PDK 的正式维护承诺。
- 当前虽已提供最小 `pdk-validate` CLI 入口，但它只覆盖结构级校验，仍不足以替代样例回归、flow 验证和人工审核。
- 当前 characterization `arc_delays` 仍是过渡设计；虽然 compound-cell characterization 现在会补一层 wildcard 输出弧以便 STA 在不同 consumer sink 名称下复用输出端 delay，但整体语义仍未收敛成商业级、可移植的正式库时序模型。

建议：

- 所有自定义 PDK 在用于正式评估前，必须执行单独验证与样例回归。

## 9. 外部依赖限制

- 当前外部依赖正式支持范围仍然很窄，主要限于 Rust stable、`uv`、`maturin` 和受限的 `josim` / `josim-cli` 调用路径。
- 其他外部仿真器、自定义命令包装、非标准安装方式和系统环境差异，当前都不构成正式支持承诺。

建议：

- 对外部依赖相关问题保留版本号、安装来源、执行命令和平台信息。
- 对 `external_josim` 相关问题同时保留 deck、内部模式结果和外部对照结果。

## 10. 平台与分发限制

- 当前默认 CI 已覆盖 Ubuntu 主检查、Windows 核心 smoke，以及 Windows waveform parity gate；但仍缺 macOS 与 Linux waveform same-platform baseline 的完整矩阵闭环。
- 平台验证仍以最小核心链路为主，尚未形成 Ubuntu/Windows/macOS 对称的全流程质量门。
- 仓库尚未提供正式预编译 wheel 和 CLI 二进制对外发布承诺；当前 `release-artifacts-optional` 仍用于内部评审用的当前 runner wheel / CLI bundle，而不是正式发布渠道。

建议：

- 目前以源码构建和内部环境固定化为主。

## 11. 产品化限制

- 当前错误码体系尚未全量统一；目前 `rflux-io` 输入 / schema 错误、未分类 CLI 失败兜底、部分 `FLOW` 上下文失败、`simulate-file` 的缺失输入 / 部分受限语法失败，以及 `check-equivalence` 的接口不匹配 / 顺序语义不支持失败已开始接入稳定 `RFLOW-*` 错误码，但更多 `VERIFY` 与 flow / sim 内部错误仍需继续收敛。
- 当前 schema 兼容性策略尚未完全固化。
- 当前已具备最小 CLI 诊断包导出能力，并已在 `simulate-file`、`verify-layout`、`compile-layout`、`analyze-timing`、`compile-netlist`、`solve-dimacs`、`check-equivalence` 与 `lint-input` 路径上接通统一执行入口；当前已覆盖工作目录、最小环境名快照、路径/仿真配置回显、输入契约快照、已有/自动生成 JSON report 摘要，以及最小运行摘要与执行过程结构化日志；但其他真实业务命令、完整实时日志、自动日志归档、升级回滚与完整支持流程仍在建设中。

建议：

- 在对外试用前，同时提供支持矩阵、版本说明和已知限制说明。

## 12. 何时可以删除某条限制

只有当以下条件全部满足时，某条限制才可以从本文件移除：

- 代码实现已完成。
- 有稳定回归测试。
- 已进入 CI 或正式验证流程。
- 已更新支持矩阵。
- 已更新对外文档与示例。
