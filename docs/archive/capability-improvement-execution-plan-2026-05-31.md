# rflux 能力提升推进清单（启动版，2026-05-31）

## 1. 目标

把“能力建议”转换为可执行任务卡，优先推进三条主线：

- A 线：约束驱动闭环优化（analyze -> fix -> rerun）
- B 线：多角多场景（MCMM）最小可用实现
- C 线：诊断系统根因分层与复现建议

本清单强调两点：

- 先做可验证最小切片，再扩展能力边界。
- 每个任务必须绑定验收标准和 CI 锚点。

## 2. 执行原则

1. CLI First，不引入必需常驻服务。
2. 维持现有契约稳定，新增字段采用向后兼容方式。
3. 每项任务都要提供证据产物（报告、日志、基线差异或诊断包）。
4. 优先复用现有命令、脚本、基线与 review bundle 结构。

## 3. 两周启动节奏

### Week K1（启动周）

目标：把三个能力主线分别落一个最小切片并进入 CI。

交付：

- A1：闭环优化最小控制面（单次迭代）
- B1：MCMM 输入契约最小版（双 corner）
- C1：诊断根因分类 v1（5 类）

### Week K2（收敛周）

目标：把 K1 切片接到 run-with-diagnostics 与候选评审证据链。

交付：

- A2：闭环优化多轮收敛（上限 3 轮）
- B2：MCMM 汇总报告（worst-per-metric）
- C2：诊断建议模板与失败复现命令

## 4. 任务卡（启动批次）

### A1. 闭环优化最小控制面

范围：

- 在 flow 路径新增一个可选参数 `optimize_closure_rounds`（默认 0，表示禁用）。
- 当值为 1 时，执行 analyze -> fix -> rerun 的最小闭环一次。

验收标准：

- 默认行为不变（参数不传时结果与现状一致）。
- 传入 `optimize_closure_rounds=1` 时，输出报告新增 `closure_rounds_executed` 字段。
- 失败时错误码和诊断包可定位到闭环阶段（analyze/fix/rerun）。

建议 CI 锚点：

- `uv run cargo test -p rflux-flow -- --nocapture`
- `uv run cargo test -p rflux-cli run_with_diagnostics_ -- --nocapture`
- `uv run pytest python/tests/test_basic.py -k "compile_layout or analyze_timing" -q`

证据产物：

- run-with-diagnostics 报告中出现 `closure_rounds_executed`。

### B1. MCMM 输入契约最小版（双 corner）

范围：

- 新增最小 MCMM 输入结构：`corners=[...]`，每个 corner 含名字与时序参数覆盖。
- 首版仅要求支持 2 个 corner，输出每个 corner 的 timing 摘要与全局 worst summary。

验收标准：

- 旧输入（无 corners）仍可运行。
- 双 corner 输入可生成 `timing_by_corner` 与 `worst_corner` 字段。
- corner 名称冲突、空 corner 列表有稳定错误语义。

建议 CI 锚点：

- `uv run cargo test -p rflux-timing -- --nocapture`
- `uv run cargo test -p rflux-flow run_with_diagnostics_executes_analyze_timing_ -- --nocapture`
- `uv run pytest python/tests/test_basic.py -k "analyze_timing" -q`

证据产物：

- timing JSON 报告包含 `timing_by_corner` 和 `worst_corner`。

### C1. 诊断根因分类 v1

范围：

- 在现有 diagnostics 结构中引入 `root_cause_category`（输入契约、PDK、仿真外部工具、算法限制、内部错误）。
- 为每类增加最小 `next_step` 建议文本。

验收标准：

- 至少 5 类错误可稳定映射到上述分类。
- `collect-diagnostics` 与 `run-with-diagnostics` 产物中都可见分类字段。
- 不影响现有错误码语义与旧字段消费。

建议 CI 锚点：

- `uv run cargo test -p rflux-cli run_collect_diagnostics_ -- --nocapture`
- `uv run cargo test -p rflux-cli run_with_diagnostics_records_failures_in_bundle -- --nocapture`
- `uv run pytest python/tests/test_basic.py -k "verify_layout or simulate_file" -q`

证据产物：

- 诊断包 `manifest.json` 包含 `root_cause_category` 与 `next_step`。

## 5. 任务卡（第二批）

### A2. 闭环优化多轮收敛

- 新增 `closure_stop_reason`（met_target / no_improvement / max_rounds / failed）。
- 默认轮次上限 3，支持通过参数覆盖。

### B2. MCMM 汇总报告

- 新增全局汇总：`worst_setup_slack`、`worst_hold_slack`、`worst_corner_per_metric`。
- 汇总需进入候选发布证据链。

### C2. 诊断复现建议

- 输出最小复现命令块（CLI 命令 + 必需输入路径说明）。
- 与支持文档模板字段对齐。

## 6. 风险与回滚

1. 风险：新字段影响既有 JSON 消费方。
- 缓解：仅追加字段，不修改旧字段语义。

2. 风险：闭环优化引入运行时波动。
- 缓解：默认关闭；仅在显式参数开启时生效。

3. 风险：MCMM 切片扩大过快导致验证不足。
- 缓解：首版强约束为双 corner 最小集合，不一次性扩展完整场景系统。

## 7. 关联文档

- [docs/professional-capability-roadmap-v1.md](docs/professional-capability-roadmap-v1.md)
- [docs/commercialization-roadmap.md](docs/commercialization-roadmap.md)
- [docs/full-alignment-plan.md](docs/full-alignment-plan.md)
- [docs/diagnostics.md](docs/diagnostics.md)
- [docs/release-artifact-readiness-checklist.md](docs/release-artifact-readiness-checklist.md)

## 8. 启动记录

- 启动日期：2026-05-31
- 当前状态：已立项，等待按 A1/B1/C1 开始开发与回归落地
