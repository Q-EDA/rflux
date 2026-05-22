# rflux 责任矩阵 v0

## 1. 目的

本文件定义商业化推进过程中的职责分工。当前仓库未必已有完整人员编制，因此本文件先定义“角色”，后续再映射到具体负责人。

## 2. 核心角色

### 2.1 核心内核负责人

负责：

- `ir`
- `synth`
- `place`
- `route`
- `timing`
- `verify`
- `sim`

要求：

- 对算法结果正确性负责。
- 对核心回归和 benchmark 负责。

### 2.2 CLI / Python / 产品化负责人

负责：

- `cli`
- `py`
- `python/rflux`
- 安装、打包、发布、兼容策略

要求：

- 对公共接口稳定性负责。
- 对用户入口文档和示例负责。

### 2.3 PDK / 工艺负责人

负责：

- `tech`
- characterization workflow
- PDK schema
- PDK validate

要求：

- 对工艺数据质量和接入流程负责。

### 2.4 QA / 基准负责人

负责：

- CI
- nightly
- regression
- fuzz
- benchmark
- correlation

要求：

- 对质量门是否阻断发布负责。

### 2.5 文档与支持负责人

负责：

- README 与用户文档
- release notes
- 已知限制
- 支持流程与 issue triage

要求：

- 对对外承诺的一致性负责。

## 3. 决策矩阵

| 事项 | DRI | 必要评审方 |
|------|-----|------------|
| 公共 CLI 变更 | CLI / Python / 产品化负责人 | QA、文档 |
| Python facade 变更 | CLI / Python / 产品化负责人 | QA、核心内核 |
| IR / PDK schema 变更 | 核心内核或 PDK 负责人 | QA、产品化、文档 |
| 算法或 QoR 变化 | 核心内核负责人 | QA |
| PDK 新接入 | PDK 负责人 | QA、产品化 |
| 发布 | 产品化负责人 | QA、文档、相关模块 DRI |
| 已知限制收缩或扩张 | 文档与支持负责人 | 相关 DRI |

## 4. 当前执行建议

如果团队规模有限，至少也要明确以下三个人或三类职责：

- 一个对内核结果负责。
- 一个对接口和发布负责。
- 一个对质量门和 benchmark 负责。

如果这三类职责仍由同一人承担，也必须在文档上区分，否则后续无法建立发布审查机制。