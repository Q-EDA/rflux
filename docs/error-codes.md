# rflux 错误码规范

## 1. 目的

本文件定义 `rflux` 的统一错误码分层与命名规范，用于 Rust 内核、CLI、PyO3 和 Python facade 的一致化错误报告。

当前仓库尚未完成全量错误码改造。本文件先定义目标框架，后续实现必须向此处收敛。

## 2. 错误码设计原则

- 每个对外可见错误都应有稳定错误码。
- 错误码必须可机读、可搜索、可聚合。
- 错误文本可以优化，但错误码不应随意变化。
- 不能把“不支持”伪装成“内部错误”。
- 不能把“用户输入错误”伪装成“算法失败”。

## 3. 错误码格式

格式：`RFLOW-<域>-<编号>`

示例：

- `RFLOW-INPUT-001`
- `RFLOW-PDK-003`
- `RFLOW-SIM-014`

其中：

- `<域>` 表示错误域。
- `<编号>` 为三位或更多顺序编号。

## 4. 错误域定义

| 域 | 含义 |
|----|------|
| `INPUT` | 用户输入、文件格式、参数错误 |
| `SCHEMA` | JSON / report / PDK schema 不兼容 |
| `PDK` | PDK、library、characterization 配置问题 |
| `FLOW` | 综合、布局、布线、时序等流程执行问题 |
| `VERIFY` | 等价性、结构一致性与验证问题 |
| `SIM` | 仿真输入、仿真求解、外部仿真器调用问题 |
| `LIMIT` | 当前能力边界不支持 |
| `INTERNAL` | 内部未预期错误 |
| `ENV` | 环境、依赖、工具链、平台问题 |

## 5. 错误等级建议

| 等级 | 含义 | 预期动作 |
|------|------|----------|
| `error` | 当前命令失败 | 立即返回非零退出码或抛异常 |
| `warning` | 命令可继续，但结果有风险 | 输出警告并进入日志 / 报告 |
| `info` | 边界提示或模式提示 | 输出说明，不改变状态 |

## 6. 首批标准错误码

### 6.1 输入与 schema

| 错误码 | 含义 | 典型触发 |
|--------|------|----------|
| `RFLOW-INPUT-001` | 缺少输入文件 | CLI `--input` 指向不存在路径 |
| `RFLOW-INPUT-002` | 输入格式非法 | JSON / deck / DIMACS 解析失败 |
| `RFLOW-INPUT-003` | 参数冲突 | 同时给出不兼容参数组合 |
| `RFLOW-INPUT-004` | 参数缺失 | 当前模式需要的参数未提供 |
| `RFLOW-SCHEMA-001` | schema 版本不兼容 | 旧版或未来版 schema 无法解析 |
| `RFLOW-SCHEMA-002` | 缺少必填字段 | JSON 输入不满足契约 |
| `RFLOW-SCHEMA-003` | schema 类型不匹配 | 把 PDK 文件当成 IR，或把 report 当成输入文件 |

当前已开始落地的具体映射：

- `rflux-io::IoError::UnsupportedSchemaVersion` -> `RFLOW-SCHEMA-001`
- `rflux-io::IoError::InvalidJsonEnvelope` -> `RFLOW-SCHEMA-002`
- `rflux-io::IoError::UnexpectedJsonKind` -> `RFLOW-SCHEMA-003`
- `rflux-io::IoError::Io` -> `RFLOW-INPUT-001`
- `rflux-io::IoError::Json` -> `RFLOW-INPUT-002`

### 6.2 PDK 与工艺

| 错误码 | 含义 | 典型触发 |
|--------|------|----------|
| `RFLOW-PDK-001` | PDK 文件不存在或不可读 | `--pdk` 路径错误 |
| `RFLOW-PDK-002` | PDK schema 非法 | JSON 字段缺失或类型错误 |
| `RFLOW-PDK-003` | PDK 能力不足 | 当前 PDK 缺少所需 timing / routing / tech 数据 |
| `RFLOW-PDK-004` | characterization 数据不一致 | 库时序 / 工艺数据相互冲突 |

### 6.3 Flow

| 错误码 | 含义 | 典型触发 |
|--------|------|----------|
| `RFLOW-FLOW-001` | 综合失败 | 输入网表违反内部约束 |
| `RFLOW-FLOW-002` | 放置失败 | 无法满足固定节点 / blockage / 资源限制 |
| `RFLOW-FLOW-003` | 布线失败 | 当前规则下无法完成合法路由 |
| `RFLOW-FLOW-004` | 时序分析失败 | 时序图或约束不满足前置条件 |
| `RFLOW-FLOW-005` | 结果未收敛 | 迭代式流程超出收敛预算 |

### 6.4 Verify

| 错误码 | 含义 | 典型触发 |
|--------|------|----------|
| `RFLOW-VERIFY-001` | 等价检查输入非法 | 比较对象格式不匹配 |
| `RFLOW-VERIFY-002` | 当前顺序语义不支持 | 超出当前 `Dff` / `DffEnable` 子集 |
| `RFLOW-VERIFY-003` | 结构规则违反 | 布局或 PTL 结构检查失败 |

### 6.5 仿真

| 错误码 | 含义 | 典型触发 |
|--------|------|----------|
| `RFLOW-SIM-001` | deck 解析失败 | SPICE / JoSIM 子集语法不合法 |
| `RFLOW-SIM-002` | 当前 deck 语法不支持 | 子集外语法 |
| `RFLOW-SIM-003` | 当前器件模型不支持 | 超出当前内部 transient 器件集合 |
| `RFLOW-SIM-004` | 外部仿真器不可用 | `josim` 不在 PATH 或调用失败 |
| `RFLOW-SIM-005` | 仿真未收敛 | 内部 transient 步进或非线性迭代失败 |
| `RFLOW-SIM-006` | 波形输出无效 | 输出文件不存在或格式异常 |

### 6.6 能力边界与内部问题

| 错误码 | 含义 | 典型触发 |
|--------|------|----------|
| `RFLOW-LIMIT-001` | 当前版本不支持该能力 | 明确超出支持矩阵 |
| `RFLOW-LIMIT-002` | 当前规模超限 | benchmark / 目标规模外输入 |
| `RFLOW-ENV-001` | Python / Rust 工具链不满足要求 | 版本不匹配 |
| `RFLOW-ENV-002` | 外部依赖缺失 | `uv` / `maturin` / 外部 simulator 缺失 |
| `RFLOW-INTERNAL-001` | 未分类内部错误 | 仅作为临时兜底，必须逐步消灭 |

## 7. 对外呈现规范

### CLI

CLI 错误输出应至少包含：

- 错误码。
- 简要摘要。
- 失败对象或路径。
- 下一步建议。

当前推进注记：

- `rflux-cli` 已开始在 `rflux-io` 输入 / schema 错误路径上输出 `error[RFLOW-...]` 风格消息，并附带 `detail` 与 `next` 建议。
- `rflux-cli` 与 `run-with-diagnostics` 已不再使用未文档化的 `RFLOW-CLI-UNCLASSIFIED` 兜底码，普通未分类失败现统一回落到 `RFLOW-INTERNAL-001`。
- `FLOW` 路径已开始接线到同一结构化出口：当前 `compile-netlist failed` 会映射到 `RFLOW-FLOW-001`，`analyze-timing failed` 会映射到 `RFLOW-FLOW-004`，并附带稳定建议动作。
- `SIM` 路径也已开始接线到同一结构化出口：当前 `simulate-file` 缺失输入 deck 会映射到 `RFLOW-INPUT-001`，而已知不支持的 deck 语法失败已开始映射到 `RFLOW-SIM-002`。
- `VERIFY` 路径也已开始接线到同一结构化出口：当前 `check-equivalence --kind combinational` 遇到顺序网表时，会稳定映射到 `RFLOW-VERIFY-002`，并建议切换到 `single_step_sequential` 或缩小到组合子集。
- `VERIFY` 路径中的接口边界失败也已开始接线：当前 `check-equivalence` 遇到输入 / 输出 / 状态接口集合不一致时，会稳定映射到 `RFLOW-VERIFY-001`，并建议先对齐比较对象的命名接口。
- `verify-layout` 的命令级失败也已开始接线：当前 `verify-layout failed` 会稳定映射到 `RFLOW-VERIFY-003`，并建议先查看 verification report 或诊断包中的结构/仿真校验细节。
- 其他错误域当前仍需要继续把 `FLOW` / `SIM` / `VERIFY` 路径接到同一结构化出口，尤其是模块内仍直接暴露原始 I/O 或 anyhow 错误的路径。

示例：

```text
error[RFLOW-SIM-002]: unsupported deck syntax for internal_transient
  file: python/tests/benchmarks/example.cir
  detail: .measure is not supported in the current parser subset
  next: run with --mode external_josim or remove unsupported statements
```

### Python

Python facade 应抛出统一异常类型，并至少提供：

- `code`
- `message`
- `details`
- `suggestion`

### JSON 报告

当命令以 JSON 输出失败信息时，建议结构为：

```json
{
  "ok": false,
  "error": {
    "code": "RFLOW-SIM-002",
    "message": "unsupported deck syntax for internal_transient",
    "details": {
      "statement": ".measure tran ..."
    },
    "suggestion": "Use external_josim mode or remove unsupported statements."
  }
}
```

## 8. 落地要求

后续代码改造必须满足：

- 新增公共错误路径时，同时补本文档。
- 新增错误码时，同时补最小回归测试。
- 不允许长时间以 `RFLOW-INTERNAL-001` 兜底替代精确分类。
- 对已在 Rust 中实现错误分类的模块，必须至少暴露“错误码 + 建议动作”两个稳定字段，供 CLI / Python 后续接线复用。

## 9. 当前推进优先级

优先收敛以下错误域：

1. `SIM`
2. `INPUT`
3. `PDK`
4. `FLOW`
5. `LIMIT`

原因是这些域最直接影响 CLI / Python / 对外试用体验。