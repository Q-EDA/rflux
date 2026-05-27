# rflux 专业能力提升路线图 v1（6 周执行版）

## 1. 目标

本路线图聚焦“短周期专业化提升”，不追求功能面扩张，优先提升以下能力：

- 契约稳定性（CLI / Python / JSON schema）
- 质量门可信度（跨平台 + 分层回归）
- 结果可信度（对照与误差预算）
- 可支持性（诊断包、日志、证据链）
- 发布可审计性（准入门、变更记录、回滚）

适用周期：6 周

## 2. 成功标准（6 周末）

1. 任何公共接口变更都能被自动检测并出具差异报告。
2. 核心路径至少具备 Ubuntu + Windows 双平台最小 CI smoke。
3. timing / sim / verify 各有一组可持续运行的对照基线与阈值解释。
4. 诊断包可直接用于支持工单，具备完整 stdout/stderr 与最小环境指纹。
5. 候选发布具备统一 evidence 清单，go / no-go 决策可追溯。

## 3. 优先级与工作流

优先级从高到低：

- P0：契约治理 + 质量门
- P1：可信度与对照
- P2：支持与发布运维

执行节奏：

- 每周 1 次周目标评审
- 每周 1 次风险复盘（仅处理阻塞项）
- 所有任务以“可验收证据”收尾

## 4. 周计划

### Week 1：契约基线自动化（P0）

交付：

- 新增契约快照资产：
  - CLI 命令/参数面快照
  - Python public surface 快照（顶层 + 子模块）
  - 关键 JSON report schema 快照
- 新增 CI 检查：检测破坏性差异并输出变更摘要。

验收：

- 人为改动 1 个公共参数名可触发 CI 失败。
- PR 页面可看到“新增/删除/默认值变化”的机器可读差异。

建议落点：

- `crates/cli`：命令面导出辅助
- `python/rflux`：public symbols 导出辅助
- `.github/workflows/ci.yml`：契约差异门

### Week 2：跨平台最小质量门（P0）

交付：

- 扩展 CI 矩阵：Ubuntu + Windows 的核心 smoke。
- 核心 smoke 覆盖：
  - CLI 最小链路（lint-input / compile-netlist / check-equivalence）
  - Python 最小链路（`python/tests/test_basic.py` 子集）

当前进展（本仓库现状）：

- 已新增 `core-smoke-windows` job，直接覆盖上述 CLI / Python 最小链路。
- Ubuntu 默认 `checks` job 继续作为主回归门；当前状态为“Ubuntu 全量 + Windows 最小 smoke”。

验收：

- 同一提交在双平台 smoke 全绿。
- 失败日志可直接定位到命令级别。

### Week 3：结果可信度基线（P1）

交付：

- 选定 1 组 timing、1 组 verify、1 组 sim 黄金样例。
- 为每组建立“偏差阈值 + 解释文档 + 趋势记录”。

验收：

- 引入人工退化可被阈值门捕获。
- 对照结果 artifact 可从 CI 下载并复核。

建议落点：

- `docs/josim-parity.md`
- `docs/sim-release-readiness-checklist.md`
- 相关 pytest/cargo 回归集合

当前进展（本仓库现状）：

- 已新增 `python/scripts/summarize_quality_baseline_results.py`，用于按阈值清单汇总 timing / verify / sim 指标并输出 JSON + Markdown 评审摘要。
- 已新增阈值清单 `python/tests/benchmarks/week3/quality_thresholds.json`、黄金样例 `python/tests/benchmarks/week3/quality_results.golden.json`，以及单测 `python/tests/test_quality_baseline_summary_utils.py` / `python/tests/test_quality_baseline_summary_runner.py`。
- 已新增 `python/tests/benchmarks/week3/quality_summary.approved-baseline.json`，用于 `--previous-summary-json` no-regression 校验。
- 主 CI 已新增显式 smoke anchor：
  - `uv run pytest python/tests/test_quality_baseline_summary_utils.py -q`
  - `uv run pytest python/tests/test_quality_baseline_summary_runner.py -q`
  - `uv run python python/scripts/summarize_quality_baseline_results.py --results-json python/tests/benchmarks/week3/quality_results.golden.json --summary-json target/week3-quality/quality_summary.current.json --summary-md target/week3-quality/quality_summary.current.md --validate-pass`
  - `uv run python python/scripts/summarize_quality_baseline_results.py --results-json python/tests/benchmarks/week3/quality_results.golden.json --summary-json target/week3-quality/quality_summary.current.with-history.json --summary-md target/week3-quality/quality_summary.current.with-history.md --previous-summary-json python/tests/benchmarks/week3/quality_summary.approved-baseline.json --validate-no-regression --regression-tolerance 0.0`

### Week 4：诊断与支持闭环（P2）

交付：

- 诊断包增强：
  - 完整 stdout/stderr 归档
  - 最小环境指纹（脱敏）
  - 失败摘要标准化
- 支持工单模板：最小复现输入、版本、平台、诊断包路径。

验收：

- 对 2 类典型失败（输入契约失败、模拟失败）可一包复现。
- 支持模板可在无口头补充情况下完成首次分诊。

建议落点：

- `docs/diagnostics.md`
- `docs/defect-severity-sla.md`

### Week 5：发布证据链与准入门（P2）

交付：

- 候选发布 evidence 统一生成：
  - 测试摘要
  - 契约差异摘要
  - 关键对照结果
  - 产物与 manifest
- go / conditional / no-go 判定模板落地。

验收：

- 一次候选发布评审可依据 evidence 独立复核。
- 准入门与回滚条件在文档中可追溯。

建议落点：

- `docs/release-policy.md`
- `docs/release-artifact-readiness-checklist.md`

### Week 6：收敛与运营化（P0/P1/P2 汇总）

交付：

- 汇总前 5 周增量，形成 v1 运营基线：
  - 指标面板（通过率、退化数、兼容破坏数）
  - 风险清单（未关闭阻塞项）
  - 下一季度路线图输入

验收：

- 能回答三个问题：
  - 现在支持边界是什么？
  - 本版本可信到什么程度？
  - 出问题时如何定位和回滚？

## 5. KPI（建议）

- 契约破坏漏检率：目标 0
- 双平台核心 smoke 通过率：>= 95%
- 关键对照回归失败平均修复时长：< 3 天
- 支持工单首轮可复现率：>= 85%
- 候选发布 no-go 原因可追溯率：100%

## 6. 风险与缓解

1. 风险：功能开发挤占治理工作。
- 缓解：P0 任务进入发布阻断条件，未完成不允许升候选。

2. 风险：Windows 差异导致 CI 波动。
- 缓解：先最小 smoke，逐步扩面，不一次性铺满。

3. 风险：对照阈值难以一次定准。
- 缓解：先固定样例与解释，再收敛阈值，不追求首周“完美阈值”。

## 7. 与现有文档的关系

- 长周期战略仍以 [docs/commercialization-roadmap.md](docs/commercialization-roadmap.md) 为准。
- 本文是“可执行短周期切片”，用于把战略转为 6 周交付。
- 发布评审继续使用 [docs/release-artifact-readiness-checklist.md](docs/release-artifact-readiness-checklist.md)。
