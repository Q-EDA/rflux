# 诊断包基线

当前仓库已提供最小 CLI 诊断包导出能力，用于把单次问题复现所需的核心上下文收集到一个可归档目录中。

当前还提供统一执行入口 `run-with-diagnostics`，可在执行真实业务命令的同时直接产出诊断包；现阶段已接通 `simulate-file`、`verify-layout`、`compile-layout`、`analyze-timing`、`compile-netlist`、`solve-dimacs`、`check-equivalence`、`lint-input` 和 `pdk-validate`。

这些命令家族当前已有显式 CI smoke 锚点，而不只是隐含地落在 `cargo test --workspace` 里：

- `collect-diagnostics`：`cargo test -p rflux-cli run_collect_diagnostics_writes_manifest_and_copies_inputs -- --nocapture`
- `run-with-diagnostics` 各业务入口：`cargo test -p rflux-cli run_with_diagnostics_ -- --nocapture`

## 命令

```bash
cargo run -p rflux-cli -- collect-diagnostics \
  --output-dir target/diagnostics/example \
  --command simulate-file \
  --input path/to/example.ir.json \
  --pdk path/to/example.pdk.json \
  --report target/reports/simulate-report.json \
  --mode internal_transient \
  --external-command josim \
  --notes "capture for support reproduction"
```

```bash
cargo run -p rflux-cli -- run-with-diagnostics \
  --output-dir target/diagnostics/sim-run \
  --kind simulate-file \
  --input path/to/example.cir \
  --mode internal_transient
```

```bash
cargo run -p rflux-cli -- run-with-diagnostics \
  --output-dir target/diagnostics/verify-run \
  --kind verify-layout \
  --input path/to/example.ir.json \
  --pdk path/to/example.pdk.json \
  --mode internal_transient
```

```bash
cargo run -p rflux-cli -- run-with-diagnostics \
  --output-dir target/diagnostics/compile-run \
  --kind compile-layout \
  --input path/to/example.ir.json \
  --pdk path/to/example.pdk.json
```

```bash
cargo run -p rflux-cli -- run-with-diagnostics \
  --output-dir target/diagnostics/timing-run \
  --kind analyze-timing \
  --input path/to/example.ir.json \
  --pdk path/to/example.pdk.json
```

`compile-layout` 报告的 `timing.closure` 与 `analyze-timing` 报告的 `closure` 是 timing closure 的机器可读入口。`status == "closed"` 表示 setup / hold / SFQ capture-window 均无 violation；`status == "open"` 时查看 `failing_checks`、`setup_violations`、`hold_violations`、`capture_window_violations` 和 `next_step`，再结合负 slack 的 timing arcs 继续修约束、SFQ 相位/脉冲窗口或物理实现。

`closure.action_count`、`closure.primary_action` 与 `closure.action_summary` 给上层调度器提供稳定入口：先处理 `primary_action`，再按 `reduce_route_delay`、`relax_constraint_or_improve_library_timing`、`add_hold_padding`、`adjust_sfq_phase_or_pulse_window` 的计数决定是重跑布线、放宽约束/补库时序、进入 hold padding 路径，还是调整 SFQ 相位分配或脉冲捕获窗口。

`compile-layout` 报告还会在 `timing.closure_loop` 中记录物理闭环步骤：detour feedback 和 hold-fix reroute 是否尝试、是否应用、detour overhead 与 hold violation 的前后变化、闭环 `status` 与 `next_step`。如果 `closure.action_summary.add_hold_padding > 0`，可以用 `compile-layout --min-hold-jtl-length-um <um>` 或 `run-with-diagnostics --kind compile-layout --min-hold-jtl-length-um <um>` 触发保守的 hold padding reroute。

如果 `closure.action_summary.reduce_route_delay > 0`，`timing.closure_loop` 会给出 `recommended_prefer_ptl_from_length_um`、`recommended_detour_margin_um`、`recommended_route_mode`、`estimated_route_length_um` 和 `estimated_slack_deficit_ps`。`compile-layout` 还会用推荐 routing 参数做一次非破坏性的 candidate rerun，并输出 `candidate_worst_setup_slack_ps`、`candidate_setup_violations`、`candidate_hold_violations`、`candidate_route_mode`、`candidate_route_length_um` 和 `reduce_route_delay_candidate_improved`。上层可据此决定是否用 `compile-layout --prefer-ptl-from-length-um <um> --detour-margin-um <um>` 固化该候选。

`closure.actions` 会列出当前最该处理的 violation arc。每个 action 包含 `check`、`priority`、`remediation_kind`、`from`、`to`、`slack_ps`、`route_mode`、`route_length_um` 和 action-level `next_step`；setup action 优先提示缩短 arrival / route delay 或调整 required time，hold action 优先提示增加最小路径延迟或检查 hold-fix reroute，capture-window action 优先提示检查 domain phase offset 并调整 SFQ 相位或 pulse window。

```bash
cargo run -p rflux-cli -- run-with-diagnostics \
  --output-dir target/diagnostics/netlist-run \
  --kind compile-netlist \
  --input path/to/example.ir.json \
  --pdk path/to/example.pdk.json
```

```bash
cargo run -p rflux-cli -- run-with-diagnostics \
  --output-dir target/diagnostics/dimacs-run \
  --kind solve-dimacs \
  --input path/to/example.cnf \
  --assumptions 1,-2
```

```bash
cargo run -p rflux-cli -- run-with-diagnostics \
  --output-dir target/diagnostics/equivalence-run \
  --kind check-equivalence \
  --input path/to/lhs.ir.json \
  --rhs path/to/rhs.ir.json \
  --equivalence-kind combinational \
  --dimacs-output target/diagnostics/equivalence-run/equivalence.cnf
```

SFQ timing reports now include pulse-phase context on each timing arc: `launch_phase`,
`capture_phase`, `launch_window_start_ps`, `launch_window_end_ps`,
`capture_window_start_ps`, `capture_window_end_ps`, `arrival_phase_offset_ps`,
`capture_window_slack_ps`, and `capture_window_violation`. Summary reports also
include `capture_window_violations`, giving schedulers a direct count of SFQ
pulse-window misses alongside setup and hold violations.

Sequential equivalence can also be run as a bounded small-system check:

```bash
cargo run -p rflux-cli -- check-equivalence \
  --lhs path/to/lhs.bench \
  --rhs path/to/rhs.bench \
  --kind bounded_sequential \
  --depth 3
```

The bounded report contains `depth`, `checked_steps`, `unroll_mode`,
`first_failing_step`, aggregate SAT stats, and per-step
`single_step_sequential` reports.
`unroll_mode == "state_unrolled"` means each time frame has distinct input and
state variables, with next-state variables constrained into the following frame.
`run-with-diagnostics` accepts the same mode through
`--equivalence-kind bounded_sequential --equivalence-depth N`.

```bash
cargo run -p rflux-cli -- run-with-diagnostics \
  --output-dir target/diagnostics/lint-run \
  --kind lint-input \
  --input path/to/example.ir.json \
  --input-kind ir
```

```bash
cargo run -p rflux-cli -- run-with-diagnostics \
  --output-dir target/diagnostics/pdk-validate-run \
  --kind pdk-validate \
  --input path/to/example.pdk.json
```

## 当前输出内容

- `manifest.json`：诊断包元数据、平台信息、CLI 版本、调用参数、当前工作目录、最小环境摘要。
- `events.jsonl`：诊断包采集过程的结构化事件日志，当前覆盖 bundle 开始、输入复制和 manifest 生成。
- `events.jsonl`：诊断包结构化事件日志；在 `run-with-diagnostics` 路径下，当前还覆盖真实命令的开始、完成或失败事件。
- `inputs/`：按原文件名复制的输入文件副本，当前支持 `--input`、`--pdk`、`check-equivalence` 的 `lhs` / `rhs` 双输入，以及 `lint-input` / `pdk-validate` 的 PDK 契约快照。
- `reports/`：按原文件名复制的现有 JSON report，当前支持 `--report`。

`manifest.json` 中当前还会附带：

- `configuration`：标准化路径回显与仿真相关配置回显。
- `summary`：采集到的输入数量、legacy 兼容输入、契约检查失败统计。
- `execution`：真实命令执行状态、错误码、错误消息，以及最小 stdout/stderr 摘要。
- `captured_reports`：现有业务 JSON report 的复制结果，以及 `kind` / `schema_version` / 解析错误摘要。
- `structured_logs`：当前结构化日志文件路径、格式和事件数。
- `RFLOW_*` / `JOSIM_*` 已出现环境变量名清单（仅记录名称，不记录值）。
- `--input` / `--pdk` 的契约快照：versioned envelope / legacy raw JSON 路径，以及 schema version。
- JSON 检查失败时的 `inspection_error`，避免因为诊断包采集而丢失原始坏输入。

## 当前用途

- 让支持和研发拿到一份固定目录，避免只靠口头描述问题。
- 在 `simulate-file`、`verify-layout`、`compile-layout`、`analyze-timing`、`compile-netlist`、`solve-dimacs`、`check-equivalence`、`lint-input` 和 `pdk-validate` 路径下，已经可以把“执行命令”和“导出诊断包”合并成一步。
- 为后续结构化日志、运行摘要和性能 profile 开关预留统一归档位置。

## 当前限制

- `run-with-diagnostics` 当前已接通 `simulate-file`、`verify-layout`、`compile-layout`、`analyze-timing`、`compile-netlist`、`solve-dimacs`、`check-equivalence`、`lint-input` 和 `pdk-validate`，但还没有覆盖其他真实业务命令。
- 还未自动打包实际业务命令的完整实时 stdout/stderr 流；当前只有最小执行摘要和 bundle 事件日志。
- 还未导出完整配置快照或全量依赖环境；当前只覆盖路径/仿真参数回显、运行摘要、输入契约快照和已有 report 摘要。
- 还未包含性能 profile、外部工具版本探测或压缩归档封装。

因此，当前诊断包应视为“最小可复现上下文基线”，不是完整商业支持包。
