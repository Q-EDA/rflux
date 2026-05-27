# rflux 发布与兼容策略

## 1. 目的

本文件定义 `rflux` 的发布级别、版本策略、兼容性原则和回滚要求。

当前仓库仍处于 `0.x` 阶段，因此默认规则是“快速迭代优先于长期兼容”，但从本文件生效起，所有对外接口都要逐步收敛到可管理的兼容策略。

## 2. 发布级别

`rflux` 发布分为四级：

### 2.1 Dev Snapshot

适用对象：仓库开发者。

特点：

- 可频繁变更。
- 不承诺向后兼容。
- 仅要求仓库主 CI 通过。

### 2.2 Alpha

适用对象：内部验证与深度共研用户。

要求：

- 有明确支持矩阵。
- 核心流程具备 smoke + regression。
- 已知限制文档完整。

### 2.3 Beta

适用对象：受控外部试点用户。

要求：

- CLI / Python / schema 兼容策略明确。
- 有安装、升级、回滚说明。
- 关键 benchmark 与对照验证稳定运行。

### 2.4 GA

适用对象：正式商业用户。

要求：

- 有正式支持矩阵。
- 有版本化 schema 与迁移策略。
- 有缺陷分级和支持响应流程。
- 有正式发布产物与回滚方案。

## 3. 版本号策略

采用语义化版本：`MAJOR.MINOR.PATCH`

### 3.1 当前阶段规则

- `0.MINOR.PATCH` 阶段允许较快迭代。
- 但从本文件起，以下接口变更必须记录在 release notes 中：
  - CLI 参数变更
  - Python API 公共字段变更
  - IR / PDK / report schema 变更
  - 默认行为变更

### 3.2 版本升级规则

- `PATCH`：缺陷修复、性能修复、无公共契约破坏。
- `MINOR`：新增能力、可兼容字段扩展、实验能力转正式。
- `MAJOR`：非向后兼容变更、schema 重大调整、默认语义变化。

在 `0.x` 阶段，如果发生明显的公共契约破坏，也应按“准 major”处理，在 release notes 中单独标红说明。

## 4. 兼容性策略

### 4.1 必须追踪的兼容面

- CLI 子命令名称与参数。
- Python facade 公共函数与 dataclass 字段。
- IR JSON schema。
- PDK JSON schema。
- report JSON schema。
- 对外错误码。

### 4.2 兼容等级

| 等级 | 说明 |
|------|------|
| `stable` | 正式承诺兼容，仅允许向后兼容扩展 |
| `limited` | 尽量兼容，但可能随阶段推进调整 |
| `experimental` | 不承诺兼容，可能随时变更 |

当前建议：

- IR JSON：`limited`
- PDK JSON：`limited`
- report JSON：`limited`
- CLI：`limited`
- `compile(...)` 等实验接口：`experimental`

### 4.3 版本化 schema 的兼容窗口

对 IR JSON、PDK JSON、report JSON，采用以下最小兼容规则：

- 当前 writer 只写最新受支持 schema。
- 当前 reader 至少接受“当前 schema”和“上一阶段明确保留的 legacy 读取路径”。
- 当 reader 仍保留 legacy 裸 JSON 兼容时，release notes 必须明确写出该兼容仍为临时过渡策略。
- 任何移除 legacy 读取兼容的动作，都必须按“准 major”处理，即使仍处于 `0.x`。

当前仓库的即时规则：

- IR JSON / PDK JSON 官方 writer 已统一写出 `schema_version + kind + payload` 包装对象。
- reader 仍接受历史裸 JSON，以保证既有 fixture、脚本和试验数据可继续读取。
- 在移除这条 legacy 兼容前，必须先满足：
  - 有显式迁移脚本或自动升级路径。
  - 有 schema 兼容回归测试。
  - release notes 明确列出影响面与回滚方式。

## 5. 发布准入门

### 5.1 所有版本共同要求

- 主 CI 通过。
- 变更有对应测试或文档说明。
- 发布说明已生成。
- 已知限制已更新。
- 若发布涉及 PDK schema、PDK 默认行为或新接入 PDK，必须附带 `pdk-validate` 结果或等价结构校验记录。
- 主 CI 与进入发布评审的可选 workflow 必须复用仓库内共享的环境 bootstrap 路径，避免 Rust/Python/uv 初始化在不同 job 之间发生未审查漂移。

### 5.2 Alpha 额外要求

- 核心流程 smoke 全通过。
- 支持矩阵与错误码文档已更新。
- 至少一条标准安装路径经验证。
- 自定义 PDK 若进入 Alpha 范围，必须至少通过当前最小 `pdk-validate` 校验。

### 5.3 Beta 额外要求

- 关键 benchmark 无未解释重大退化。
- 关键对照矩阵持续通过。
- CLI / Python 示例全部可运行。
- 回滚流程经验证。
- 进入 Beta 范围的 PDK 变更必须同时提供 `pdk-validate` 结果、benchmark 回归和已知限制更新。

### 5.4 GA 额外要求

- 支持矩阵进入正式维护。
- schema 兼容测试稳定。
- 发布产物可复现。
- 有明确缺陷响应策略。
- GA 范围内的 PDK 准入不能只依赖 `pdk-validate`；必须把它视为结构门，外加 benchmark、flow/timing 对照和发布审查记录。

### 5.5 PDK 准入记录

凡是把某套 PDK 提升到对外试用、Beta 或 GA 范围时，发布资料中至少应保留以下记录：

- `pdk-validate` 输出报告或等价自动校验产物。
- 对应 PDK 版本、来源、负责人和支持等级。
- 关键 compile / layout / timing benchmark 回归结果。
- 已知限制和不承诺项是否有变化。

## 6. 回滚策略

每个发布版本都必须具备：

- 对应 tag。
- 对应 release notes。
- 对应构建指纹或产物记录。
- 升级失败时的回滚步骤。

回滚最少需要覆盖：

- Python 环境回滚。
- Rust CLI 二进制回滚。
- schema 迁移失败回滚。

## 7. 变更日志要求

每次发布说明至少包含：

- 新增能力。
- 修复的问题。
- 兼容性变化。
- 已知风险。
- 升级动作。
- 需要关注的 benchmark / QoR 影响。

## 8. 热修复策略

以下情况可触发热修复：

- 崩溃导致核心命令不可用。
- 报告结果存在已确认严重错误。
- 对外发布产物不可安装。
- 关键兼容性破坏影响当前试点用户。

热修复要求：

- 问题必须可复现。
- 修复必须附带最小回归。
- 必须说明是否需要用户回滚或重新生成结果。

## 9. 当前执行建议

在正式建立发布流水线前，先按以下最小机制执行：

1. 每月生成 1 个内部候选版本。
2. 所有候选版本附带 release notes 草案。
3. 所有公共契约变更必须更新文档。
4. 不允许在无说明情况下改变默认行为。
5. 如果候选版本包含 `sim`、JoSIM 对照脚本或 phase-6 阈值清单改动，必须附带 waveform compare 当前 summary、基线来源说明，以及在存在同平台 approved baseline 时的 no-regression 结论或豁免原因；若变更触及 unsupported-warning contract，也必须同时附带 external-warning review bundle（summary + manifest）。
6. 上述仿真候选版本评审应按 [sim-release-readiness-checklist.md](./sim-release-readiness-checklist.md) 留存 go / no-go 记录。
7. 文档、示例或发布门禁中出现的新命令，必须落到已有 CI smoke job 或新增受控 workflow step，不能只停留在说明文字里。
8. 候选发布产物当前应通过手动触发的 `release-artifacts-optional` workflow job 生成，统一产出当前 runner 上的 CLI 候选二进制、Python wheel、构建输入副本和 manifest，而不是临时手工拼接命令。
9. 候选发布产物评审应按 [release-artifact-readiness-checklist.md](./release-artifact-readiness-checklist.md) 留存 go / no-go 记录。

当前候选发布构建也已有显式 CI smoke anchor：

- `uv run pytest python/tests/test_prepare_release_artifacts.py -q`