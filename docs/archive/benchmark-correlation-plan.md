# rflux 基准与对照验证方案 v0

## 1. 目的

本文件定义 `rflux` 的 benchmark、相关性验证和结果可信度建设路径。它服务于两个目标：

- 持续观察性能与 QoR 漂移。
- 建立与外部工具或权威结果的长期对照。

## 2. 资产分层

`rflux` 后续应维护三层资产。

### 2.1 公共 benchmark

用途：

- 回归测试。
- 性能趋势。
- 核心流程 smoke。

来源建议：

- 当前仓库中的 synth / sat / python benchmark fixtures。
- 公开可再分发样例。

### 2.2 相关性 benchmark

用途：

- 与 Yosys / JoSIM / 手工计算 / 可信参考数据做对照。

要求：

- 输入固定。
- 参考输出固定。
- 误差阈值固定。

### 2.3 客户代表性样例

用途：

- 验证实际使用场景。
- 评估规模上限、兼容性和支持成本。

要求：

- 脱敏。
- 版本化。
- 允许长期回归。

## 3. 对照矩阵

### 3.1 综合与逻辑等价

对照目标：

- 逻辑等价结论一致性。
- DIMACS 导出与 replay 工作流稳定性。

候选对照：

- Yosys / 内部 SAT 基准。

### 3.2 物理实现

对照目标：

- placement / route 的基础 QoR 趋势。
- detour、route length、资源合法性。

候选对照：

- 手工构造基线。
- 现有 SFQ 参考流程。

### 3.3 时序

对照目标：

- worst setup / hold slack 方向一致性。
- critical path 报告稳定性。

候选对照：

- 手工计算。
- 脚本化参考实现。

### 3.4 仿真

对照目标：

- waveform correlation。
- reported delay correlation。
- violation summary correlation。

候选对照：

- JoSIM。

## 4. 关键指标

至少跟踪以下指标：

- 运行时长。
- 内存占用。
- 是否成功完成流程。
- 关键 QoR 指标。
- 与参考结果的误差。
- 误差分布和最坏案例。

## 5. 执行频率

### 每次 PR

- 运行 smoke benchmark。
- 检查是否出现明显 crash、schema 破坏或关键结果缺失。

### 每日或 nightly

- 运行完整 benchmark 套件。
- 运行相关性对照。
- 生成趋势报告。

### 每月

- 审查基线是否需要重设。
- 审查误差阈值是否仍合理。

## 6. 退出条件

后续阶段要宣称“结果可信度提升”，至少要满足：

- benchmark 资产进入版本控制。
- 关键对照矩阵可重复运行。
- 偏差超阈值会阻断发布或进入例外审查。

## 7. 当前建议落地顺序

1. 先整理当前仓库已有 benchmark 与 fixture。
2. 再定义统一结果输出格式。
3. 最后把 nightly 和 dashboard 接进 CI。