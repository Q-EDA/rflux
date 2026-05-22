# rflux 公共接口与默认行为盘点 v0

## 1. 目的

本文件用于阶段 1 的契约收敛工作，记录当前仓库中已经暴露给用户或容易形成用户依赖的接口、默认行为、placeholder 与 fallback。

该清单只记录当前代码已存在的事实，不代表这些行为都应该长期保留。

## 2. 公共接口面

### 2.1 CLI 公共命令

当前 `rflux-cli` 暴露的顶层命令：

- `pdk-minimal`
- `lint-input`
- `compile-netlist`
- `compile-layout`
- `analyze-timing`
- `verify-layout`
- `simulate-file`
- `solve-dimacs`
- `check-equivalence`

### 2.2 Python 公共 API

当前 `python/rflux/__init__.py` 通过 `__all__` 暴露的主要公共 API 包括：

- 建模与配置类型：`Circuit`、`CompilePlan`、`PinRef`、`FixedNodePlacement`、`BlockedRegion`、时序约束类型等。
- 报告类型：`CompileReport`、`SynthesisReport`、`LayoutReport`、`TimingAnalysisReport`、`VerificationReport`、`SimulationReport` 等。
- 主工作流函数：`compile`、`compile_plan`、`compile_plan_report`、`compile_netlist`、`compile_layout`、`analyze_timing`、`analyze_timing_statistical`、`verify_layout`、`check_equivalence` 等。

### 2.3 JSON / 文件接口

当前实际形成公共契约的文件接口包括：

- IR JSON 读写
- PDK JSON 读写
- CLI JSON 输出
- Python `Circuit.to_json()` / `from_json()`
- Python `Pdk.to_json()` / `from_json()`

## 3. 当前默认行为盘点

### 3.1 CLI 默认行为

- `pdk-minimal --name` 默认值为 `minimal-sfq`。
- 绝大多数 flow 命令在未指定 `--pdk` 时，会回退到 `Pdk::minimal("minimal-sfq")`。
- `verify-layout` 与 `simulate-file` 的 `--mode` 默认为 `auto`。
- `check-equivalence` 的 `--kind` 默认为 `combinational`。
- `compile-layout`、`analyze-timing`、`verify-layout` 当前直接使用 `FlowConfig::default()`。

风险说明：

- 这些默认行为当前是隐式契约，后续必须决定哪些要稳定保留，哪些要改为显式配置或至少写入 schema / 文档 / 错误提示。

### 3.2 Python 默认行为

- 多个仿真相关 API 的 `simulation_mode` 默认值为 `"auto"`。
- 多个仿真相关 API 的 `external_command` 默认值为 `None`。
- `compile_netlist` 在未给出 plan 时，会使用空 `CompilePlan()`。
- 多个 facade 在 core extension 缺失时会走 Python fallback 路径或报 `rflux extension is unavailable`。

## 4. 当前 placeholder 与 fallback 行为

### 4.1 明确 placeholder

- `compile(circuit)` 当前仍是实验性 placeholder，但已改为显式抛出 `NotImplementedError`，不再静默返回输入 `Circuit`。

结论：

- 该接口不应被视为稳定商业接口；当前状态已经避免误导性伪成功，后续仍需决定删除、保留为实验接口，或接入真实实现。

### 4.2 Python extension 缺失时的 fallback `Circuit`

当 `_core` 扩展导入失败时：

- `core_version()` 会返回 `"unavailable"`。
- `Circuit` 会退化为纯 Python stub 实现。
- 多个 `_core_*` 函数引用为 `None`。

风险说明：

- 这种行为对开发和测试便利，但对商业产品来说属于高风险隐式退化路径，必须被更明确地区分为“开发 stub”或“不可用于正式结果”的模式。

### 4.3 `compile_plan_report` / `compile_netlist` 缺失扩展时的显式失败

当 `_core_compile_plan` 或 `_core_compile_netlist` 不可用时：

- `compile_plan_report(...)` 当前会显式抛出 `RuntimeError`。
- `compile_plan(...)` 会通过 `compile_plan_report(...)` 传播该失败。
- `compile_netlist(...)` 当前会显式抛出 `RuntimeError`。

风险说明：

- 当前这条切片已经不再提供误导性的近似综合结果，并已成为后续高层 API 收敛的基线模式。

### 4.4 `analyze_timing` / `verify_layout` 缺失扩展时的显式失败

当 `_core_analyze_timing` 或 `_core_verify_layout` 不可用时：

- `analyze_timing(...)` 当前会显式抛出 `RuntimeError`。
- `verify_layout(...)` 当前会在完成 `simulation_mode` 参数校验后显式抛出 `RuntimeError`。
- 不再生成基于 `compile_layout(...)` 拼装出来的近似 timing / verification 报告。

风险说明：

- 这条切片已经从“按模式回退”收敛为“统一不可用”，显著降低了伪成功风险；统计时序接口虽然仍属研究导向，但缺失扩展时也不再由 Python facade 拼装近似报告。

### 4.5 `compile_layout` 缺失扩展时的显式失败

当 `_core_compile_layout` 不可用时：

- `compile_layout(...)` 当前会显式抛出 `RuntimeError`。
- 不再生成近似 layout / route / timing 汇总报告。

风险说明：

- 这条切片已经从“误导性伪成功”收敛为“显式不可用”，并与 `analyze_timing(...)`、`verify_layout(...)` 的当前策略保持一致。

## 5. 当前 schema 风险点

### 5.1 CLI JSON 输出已开始顶层版本化，但兼容政策仍未完整建立

当前 CLI 顶层报告已统一加入 `schema_version`，等价性 DIMACS sidecar 也带版本号；但 IR / PDK JSON 以及跨版本兼容策略仍未形成完整制度。

### 5.2 IR / PDK JSON 读写未建立兼容政策

当前 `rflux-io` 已开始把官方 IR / PDK JSON 文件写成带 `schema_version`、`kind`、`payload` 的包装对象，并对不支持的 schema version 显式拒绝；同时为了兼容既有仓库 fixture，读取路径暂时仍接受历史的裸 JSON。

### 5.3 Python 报告模型与 CLI JSON 结果模型并未显式绑定同一版本

这意味着后续必须补统一契约文档与兼容性测试。

## 6. 阶段 1 建议整改顺序

1. 先给公共接口分级：`stable`、`limited`、`experimental`。
2. 再把 placeholder 与 fallback 标记进支持矩阵和已知限制。
3. 然后为 CLI JSON / IR JSON / PDK JSON 加 schema version。
4. 最后再处理默认行为是否要显式化。

## 7. 立即可开的整改项

### 高优先级

- 明确 `compile(...)` 的去留。
- 明确无 `_core` 扩展时哪些 API 应允许 fallback，哪些应直接失败。
- 为 CLI 输出引入统一 schema version 字段。
- 为默认 `minimal-sfq` 与 `FlowConfig::default()` 建立文档和回归。

### 中优先级

- 梳理 Python facade 中哪些 dataclass 字段应视为稳定契约。
- 为仿真模式 `auto` 建立明确语义说明。
- 为 `external_command=None` 时的行为建立统一错误或回退规则。