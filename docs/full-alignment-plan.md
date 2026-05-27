# rflux 到达完全对齐推进计划（v1）

## 1. 目标与边界

本计划用于把 `rflux` 从“局部强对齐 + 部分对齐”推进到“在声明范围内完全对齐”。

这里的“完全对齐”不是指无边界覆盖所有 EDA 场景，而是指：

- 在仓库已声明的产品边界内（见 `product-scope` / `support-matrix`），
- 对标能力（Yosys/Quaigh/JoSIM 对应能力面）实现功能、契约、质量门、证据链四个层面同时闭环，
- 且可持续通过 CI 与发布评审，不依赖作者手工解释。

## 2. 当前基线（2026-05-28）

按能力面估计：

- Yosys 对应核心链路（IR/综合/SAT/等价/CLI）：约 80-85%。
- Quaigh 对应布尔优化与夹具回归：约 80-90%。
- JoSIM 对应仿真语义与相关性工程化：约 60-70%。
- 产品化与发布可审计性：约 70-80%。

总体结论：已具备“可执行研究型工具链 + 强工程化约束”，尚未达到“声明范围内完全对齐”。

## 3. 完全对齐验收定义

只有以下 6 条同时成立，才认定“到达完全对齐”：

1. 功能对齐：声明范围内的能力列表均有实现，且不存在“文档承诺但代码缺失”的条目。
2. 契约对齐：CLI/Python/report schema 契约基线全部可 `--check` 守护，破坏性改动可自动阻断。
3. 结果对齐：关键对照（含 waveform / quality baseline）具备同平台 approved baseline 与 no-regression 门。
4. 平台对齐：Ubuntu + Windows 至少形成对称的核心质量门；Linux waveform baseline 补齐并默认启用 no-regression。
5. 发布对齐：release notes + review record + artifact checklist 三件套在每次候选版本可追溯。
6. 支持对齐：诊断包、错误码、已知限制、支持矩阵相互一致，且能支撑一线复现与分诊。

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

## 4.2 阶段 B：仿真对齐短板补齐（3-6 周）

目标：把当前“partial”的 JoSIM 对齐推进到“声明范围内 fully aligned”。

交付：

- 补齐 Linux same-platform waveform approved baseline。
- 在 Linux 对应 runner 启用默认 `validate-no-regression`。
- 对 phase-6 关键 deck 类别（含 JJ/CPR/传输线/互感）建立稳定阈值与漂移解释模板。
- 将 external-warning 空合同场景纳入持续检查，避免“空即忽略”。

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
| M5 完全对齐评审 | 第 16 周 | 6 条总验收定义全部满足 |

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
- 本文用途：定义“如何达到完全对齐”与“何时算达成”。
