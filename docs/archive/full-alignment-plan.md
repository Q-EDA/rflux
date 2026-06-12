# rflux 到达完全对齐推进计划（v1）

## 1. 目标与边界

本计划用于把 `rflux` 从“局部强对齐 + 部分对齐”推进到“在声明范围内完全对齐”。

这里的“完全对齐”不是指无边界覆盖所有 EDA 场景，而是指：

- 在仓库已声明的产品边界内（见 `product-scope` / `support-matrix`），
- 对标能力（Yosys/Quaigh/JoSIM 对应能力面）实现功能、契约、质量门、证据链四个层面同时闭环，
- 且可持续通过 CI 与发布评审，不依赖作者手工解释。

### 1.1 前端集成定位（非服务化）

为支持桌面端专业设计软件前端，本项目定位为可嵌入能力层（engine），而非服务端产品。

- 本项目提供 Rust crate / Python API / CLI 三类能力出口。
- 本项目不实现常驻服务、REST 网关、多租户调度与鉴权体系。
- 本项目产出可服务化能力（进度事件、取消控制、结构化错误、诊断包），供外部厂商按其架构封装。

### 1.2 CLI First 约束（硬性）

所有前端集成相关改造必须遵守 CLI First 原则：

- 不引入“必须先启动服务”前置条件。
- 不破坏现有命令、参数、退出码、输出契约的后向兼容性。
- 新增能力以可选方式接入，不污染默认 CLI 交互体验与脚本消费路径。
- 若出现前端集成增强与 CLI 体验冲突，以 CLI 稳定性为优先。

## 2. 当前基线（2026-05-28）

按能力面估计：

- Yosys 对应核心链路（IR/综合/SAT/等价/CLI）：约 80-85%。
- Quaigh 对应布尔优化与夹具回归：约 80-90%。
- JoSIM 对应仿真语义与相关性工程化：约 60-70%。
- 产品化与发布可审计性：约 70-80%。

总体结论：已具备“可执行研究型工具链 + 强工程化约束”，尚未达到“声明范围内完全对齐”。

重评补充（2026-05-29）：

- 基于当前代码与质量门证据，Yosys 对应面可按 `82-86%` 估计。
- Quaigh 对应面可按 `86-90%` 估计。
- JoSIM 对应面可按 `72-78%` 估计。
- 产品化与发布可审计性可按 `82-88%` 估计。
- 本轮重评仍维持“距离声明范围内完全对齐尚差最后关键闭环”的结论，主阻塞不在核心功能缺失，而在跨平台基线与发布对称化收口。

## 3. 完全对齐验收定义

只有以下 7 条同时成立，才认定“到达完全对齐”：

1. 功能对齐：声明范围内的能力列表均有实现，且不存在“文档承诺但代码缺失”的条目。
2. 契约对齐：CLI/Python/report schema 契约基线全部可 `--check` 守护，破坏性改动可自动阻断。
3. 结果对齐：关键对照（含 waveform / quality baseline）具备同平台 approved baseline 与 no-regression 门。
4. 平台对齐：Ubuntu + Windows 至少形成对称的核心质量门；Linux waveform baseline 补齐并默认启用 no-regression。
5. 发布对齐：release notes + review record + artifact checklist 三件套在每次候选版本可追溯。
6. 支持对齐：诊断包、错误码、已知限制、支持矩阵相互一致，且能支撑一线复现与分诊。
7. CLI 体验对齐：前端集成增强不破坏 CLI 既有能力和体验，默认执行路径无服务依赖且保持后向兼容。

## 4. 推进阶段

## 4.1 阶段 A：对齐口径冻结（1-2 周）

目标：把“对齐到什么程度算完成”从口头判断变成机器可核验条目。

交付：

- 建立 `alignment scorecard`（Yosys/Quaigh/JoSIM/产品化四栏）。
- 为每栏定义 `must` / `should` 项与权重。
- 把每一项映射到代码入口、测试入口、CI 锚点、文档锚点。

当前落地：

- 评分卡文档已创建：`docs/alignment-scorecard.md`。
- PR 模板已接入评分卡条目引用：`.github/PULL_REQUEST_TEMPLATE.md`。
- 周报模板已创建：`docs/alignment-scorecard-weekly-template.md`。
- 首期周报样例已创建：`docs/alignment-scorecard-weekly-2026-05-28.md`。

退出条件：

- scorecard 合入仓库并在 PR 模板中可引用。
- 每个 `must` 项都能定位到至少一条自动化检查。

阶段结论（2026-05-28）：

- `已完成`。Phase A 交付（scorecard、PR 模板引用、周报模板、首期周报、J-04 机器证据锚点）已全部落地。
- `M1` 可按“完成”状态管理；后续维护动作转入周报节奏，不再作为阻塞后续阶段启动项。

## 4.2 阶段 B：仿真对齐短板补齐（3-6 周）

目标：把当前“partial”的 JoSIM 对齐推进到“声明范围内 fully aligned”。

交付：

- 补齐 Linux same-platform waveform approved baseline。
- 在 Linux 对应 runner 启用默认 `validate-no-regression`。
- 对 phase-6 关键 deck 类别（含 JJ/CPR/传输线/互感）建立稳定阈值与漂移解释模板。
- 将 external-warning 空合同场景纳入持续检查，避免“空即忽略”。

当前准备：

- 已新增 baseline 提升脚本：`python/scripts/promote_waveform_approved_baseline.py`。
- 已新增脚本测试并接入 CI smoke：`python/tests/test_promote_waveform_approved_baseline.py`。
- 已在仿真发布评审清单中加入基线提升命令锚点。
- 已新增可手动触发的 Linux waveform gate：`waveform-compare-gate-linux-optional`（workflow_dispatch）。
- Linux gate 已默认启用 no-regression（无基线时自动降级为 validate-pass 并提示），便于从 bootstrap 平滑切换到严格门。
- 已新增 Linux 基线提升操作手册：`docs/linux-waveform-baseline-promotion-playbook.md`。

当前阶段状态（2026-05-28）：

- `进行中`。已完成工具链与证据链准备，当前主阻塞为 Linux approved baseline 尚未提升。
- 下一阶段执行入口：`docs/phase-b-execution-checklist.md`。

退出条件：

- Windows 与 Linux waveform 门都能稳定运行并产出对称评审包。
- 连续 4 周无未解释的门禁抖动。

## 4.3 阶段 C：平台与发布对称化（2-4 周）

目标：把当前“最小跨平台覆盖”提升为“发布可依赖覆盖”。

交付：

- Ubuntu/Windows 关键 smoke 覆盖矩阵统一（命令、测试集、工件产出结构一致）。
- candidate release 统一跑：
  - 契约三门（CLI/Python/report schema）
  - Week3 一键质量基线门
  - release artifact helper
- release notes / review record 模板变成候选发布必填输入。

退出条件：

- 任一候选发布都可独立复核 go/no-go。
- 文档命令与 CI 命令无漂移（抽检 10 条命令，0 偏差）。

## 4.4 阶段 D：支持与运维闭环（2-3 周）

目标：让“对齐结果”可被支持团队持续消费，而非仅研发自证。

交付：

- 错误码覆盖率提升（重点补 `VERIFY` 与 flow/sim 内部错误族）。
- 诊断包规范扩展到所有核心 CLI 业务命令。
- 缺陷分级与 SLA 文档挂接到发布评审模板。

退出条件：

- 随机抽取 5 个失败样例，可仅靠诊断包完成首次分诊。
- 高优先级问题的复现材料完整率 >= 90%。

## 5. 里程碑与验收门

| 里程碑 | 预计周期 | 必须达成 |
|---|---:|---|
| M1 口径冻结 | 第 2 周 | scorecard 合入 + must 项全部可自动核验 |
| M2 仿真双平台对齐 | 第 8 周 | Windows/Linux waveform 都默认 no-regression |
| M3 发布对称化 | 第 12 周 | 候选发布证据链全自动可追溯 |
| M4 运维闭环 | 第 15 周 | 诊断/错误码/SLA 联动落地 |
| M5 完全对齐评审 | 第 16 周 | 7 条总验收定义全部满足 |

## 6. 指标体系（周跟踪）

- 契约门漏检率：目标 0。
- waveform no-regression 门稳定度：目标 >= 95%。
- 发布证据链完整率：目标 100%。
- 文档-命令漂移数：目标 0。
- 支持首轮可复现率：目标 >= 90%。

## 7. 风险与缓解

1. 风险：仿真相关阈值跨平台天然漂移。
缓解：坚持 same-platform baseline，不跨平台直接比较数值门。

2. 风险：文档更新滞后于 CI 与脚本演进。
缓解：把“文档命令抽检”纳入候选发布强制项。

3. 风险：功能开发挤占治理建设。
缓解：对齐 `must` 项未达标时，不允许提升发布级别。

## 8. 执行责任建议

- 核心算法/综合：维护 Yosys/Quaigh 对齐项。
- 仿真与对照：维护 JoSIM 对齐项与 baseline 升级。
- QA/发布：维护契约门、评审模板、artifact 审计。
- 支持/文档：维护限制说明、支持矩阵、分诊资料一致性。

## 9. 与现有文档关系

- 长周期战略：`docs/commercialization-roadmap.md`
- 6 周执行切片：`docs/professional-capability-roadmap-v1.md`
- 前端集成契约：`docs/engine-integration-contract.md`
- Phase E 执行清单：`docs/phase-e-execution-checklist.md`
- CLI 性能基线提升手册：`docs/cli-performance-baseline-promotion-playbook.md`
- 本文用途：定义“如何达到完全对齐”与“何时算达成”。

## 10. 四周加速执行计划（2026-05-29 起）

目标：在不改变既有产品边界的前提下，把当前主要缺口压缩为可验证、可发布、可运维的闭环交付。

### Week 1（J-04 收敛周）

交付：

- 产出 Linux waveform 候选基线工件并完成评审包归档。
- 执行 Linux approved baseline 提升。
- 形成 1 份可追溯 run record（含缺口与阻塞说明）。

退出条件：

- `python/tests/benchmarks/phase6/waveform_compare_summary.linux-approved-baseline.{json,md}` 已存在。
- `check_waveform_baseline_status.py --platform linux --require-ready` 通过。

### Week 2（平台对称化周）

交付：

- Ubuntu/Windows 核心 smoke 命令、测试入口、工件命名形成对称清单。
- 差异项（若存在）逐条挂 owner 与收敛日期。

退出条件：

- 抽检 10 条文档命令与 CI 命令，偏差 `0`。

### Week 3（候选发布闭环周）

交付：

- 候选发布统一执行契约三门 + Week3 质量基线门 + release artifact helper。
- release notes/review record/checklist 三件套全部自动产物可追溯。

退出条件：

- 任一候选发布均可独立复核 go/no-go。

### Week 4（支持闭环周）

交付：

- 诊断包与错误码在核心命令面完成抽样闭环。
- 缺陷分级与 SLA 在发布评审记录中形成固定字段。

退出条件：

- 随机 5 个失败样例，首次分诊仅靠诊断包可完成。

## 11. 立即执行结果（2026-05-29）

已执行：

- 运行 baseline readiness 预检：
  - `uv run python python/scripts/check_waveform_baseline_status.py --platform windows --json-output target/waveform-baseline-status/windows.2026-05-29.json`
  - `uv run python python/scripts/check_waveform_baseline_status.py --platform linux --json-output target/waveform-baseline-status/linux.2026-05-29.json`
- 运行 Phase B 工件预检：
  - `uv run python python/scripts/check_phase_b_artifact_bundle.py --artifact-dir target/waveform-compare-linux --linux-status-json target/waveform-baseline-status/linux.2026-05-29.json --json-output target/waveform-compare-linux/phase-b-artifact-check.2026-05-29.json`
- 生成 run record：
  - `docs/phase-b-run-record-2026-05-29.md`

当前结果：

- Windows baseline: ready。
- Linux baseline: not ready（缺少 linux approved baseline）。
- Phase B promotion precheck: fail（缺少 Linux waveform compare candidate/manifest 工件）。

并行推进（不依赖 Linux 环境）已执行：

- 契约三门本地校验通过：
  - `uv run python python/scripts/export_python_api_surface.py --check`
  - `uv run python python/scripts/export_cli_command_surface.py --check`
  - `uv run python python/scripts/export_report_schema_surface.py --check`
- Week3 一键质量基线门本地校验通过：
  - `uv run python python/scripts/generate_week3_golden_results.py --validate-pass --validate-no-regression --regression-tolerance 0.0`
- 候选发布辅助能力 smoke 本地校验通过：
  - `uv run pytest python/tests/test_prepare_release_artifacts.py -q`
  - `uv run pytest python/tests/test_check_release_artifact_bundle.py -q`
  - `uv run pytest python/tests/test_generate_release_review_record.py -q`
  - `uv run pytest python/tests/test_generate_release_notes.py -q`

对应记录：

- `docs/non-linux-progress-2026-05-29.md`
- `docs/platform-symmetry-audit-2026-05-29.md`
- `docs/alignment-change-summary-2026-05-29.md`

下一执行门槛：

- 需要在 workflow_dispatch 中提供可用 `josim_command_linux` 跑出 Linux waveform 工件，再进行 baseline 提升。

## 12. 前端集成能力改造计划（CLI 不降级）

目标：在不改变项目非服务化定位前提下，补齐前端可集成能力，并把 CLI 兼容与体验作为阻断门。

### 12.1 改造范围与非范围

范围：

- 定义并实现引擎级执行进度事件模型。
- 提供可取消、可超时、可观测的执行控制能力。
- 强化结构化错误与诊断包，提升前端与支持侧消费效率。
- 建立 CLI 不降级自动化门禁与回归基线。

非范围：

- 在本仓库新增任务 REST 服务或常驻网关实现。
- 在本仓库承接鉴权、租户、配额、审计、计费等服务治理功能。

### 12.2 分阶段计划

#### Phase E1（第 1-2 周）：契约冻结与兼容基线

交付：

- 新增引擎集成契约文档：输入/输出、进度事件、错误码、诊断包字段。
- 建立 CLI 命令面、参数面、输出面兼容快照。
- 建立 CLI 关键路径性能基线（冷启动、典型命令耗时）。
- 增加性能基线采集脚本执行锚点：
  - `uv run python python/scripts/capture_cli_perf_baseline.py --output target/cli-perf/cli_perf_baseline.current.json --iterations 3 --warmup 1`
- 执行入口：`docs/phase-e-execution-checklist.md`

退出条件：

- 契约文档可被测试与 CI 锚定。
- 兼容快照检查接入 CI 且默认阻断破坏性变更。

#### Phase E2（第 3-4 周）：进度与控制能力落地

交付：

- 在仿真等长任务中输出结构化进度事件（阶段、序号、进度、可选 ETA）。
- 提供执行控制句柄（取消、超时）并在关键循环可见取消状态。
- 新增默认静默策略，确保不开启增强能力时 CLI 输出行为不变。

退出条件：

- 长任务可持续产出进度事件且可被外部适配层消费。
- 取消与超时行为可复现、可测试、可诊断。

#### Phase E3（第 5-6 周）：CLI 体验守护与发布化

交付：

- 建立 CLI 体验三门：兼容门、性能门、输出门。
- 建立“增强开启/关闭”双路径回归，确保默认路径零侵入。
- 在发布评审中新增“CLI First 不降级”签核项。
- 性能门默认比较上一基线并可阻断回归：
  - `uv run python python/scripts/capture_cli_perf_baseline.py --output target/cli-perf/cli_perf_baseline.current.json --previous-baseline target/cli-perf/cli_perf_baseline.previous.json --max-regression-ratio 0.2 --fail-on-regression`

退出条件：

- 任一候选发布均可证明前端增强未破坏 CLI 体验。
- 出现兼容破坏时可被 CI 自动阻断并给出定位证据。
- 出现性能超阈值回归时可被 CI 自动阻断并产出 `target/cli-perf/` 证据工件。

### 12.3 验收门（阻断级）

1. CLI 兼容门：命令、参数、退出码、JSON 契约不得发生未声明破坏性变更。
2. CLI 输出门：默认输出不引入新增噪声；结构化进度默认不污染脚本消费 stdout。
3. CLI 性能门：关键命令性能回归在阈值内，超阈值必须附带豁免与收敛计划。
4. 非服务化边界门：任何改造不得引入“必须常驻服务”依赖。
5. 证据链门：失败场景必须产出稳定错误码与可复盘诊断包。

### 12.4 责任分工建议

- 引擎实现（Rust/Python）：实现进度事件、执行控制、错误与诊断能力。
- CLI/契约维护：维护兼容快照、输出基线、性能回归门。
- QA/发布：维护阻断门执行与候选发布签核。
- 文档/支持：维护集成契约文档、排障手册与限制说明。
