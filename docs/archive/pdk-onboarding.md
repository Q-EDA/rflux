# rflux PDK 接入规范

## 1. 目的

本文件定义 `rflux` 接入新 PDK / library 数据时的最低要求。当前目标不是把所有工艺都接进来，而是建立一条可重复、可验证、可维护的产品化接入路径。

## 2. 接入目标

新 PDK 接入至少要回答以下问题：

- `rflux` 需要哪些最小技术数据才能运行？
- 哪些能力依赖该 PDK 的哪些字段？
- 该 PDK 在当前版本下支持哪些流程，不支持哪些流程？
- 如何验证接入结果没有破坏现有 benchmark 和样例？

## 3. 当前最小要求

一个可接入的 PDK 至少应提供：

- 唯一名称与版本标识。
- schema 版本。
- 最小工艺规则数据。
- timing 所需最小参数。
- routing / PTL / JTL 相关约束数据。
- 可以驱动最小 smoke benchmark 的配置。

## 4. 接入流程

### 4.1 准备阶段

- 确认数据来源与许可。
- 确认工艺版本、负责人、发布日期。
- 确认当前 `rflux` 版本是否具备对应消费能力。

### 4.2 建模阶段

- 将原始数据映射到 `rflux` PDK schema。
- 显式记录无法映射或需要近似的字段。
- 对任何近似值，都必须记录来源和风险。

当前 Python 绑定已提供基础 cell library 查询入口，接入脚本不应再手工解析 PDK JSON 来枚举标准单元：

```python
import rflux

pdk = rflux.Pdk.from_json(payload)
print(pdk.cell_library_name)
print(pdk.cell_library_version)
print(pdk.cell_library_source)
print(pdk.cell_library_metadata())
print(pdk.cell_library_kinds())
print(pdk.cell_library_summary())
print(pdk.cell_library_entry("sfq_gate"))
print(pdk.cell_library_entries_by_kind("macro"))
```

`cell_library_metadata()` 会返回库名、版本和来源，适合作为接入脚本的库身份入口。`cell_library_version` 和 `cell_library_source` 是兼容性便捷属性；旧 PDK JSON 缺少这些字段时会返回 `None`，新接入的库建议显式填写。`CellLibrarySummary` 会返回 cell/kind 总数、每类 kind 的库存数量、named/kind/missing timing 数量和带 characterization metadata 的 cell 数量。`CellLibraryEntry` 会返回 cell 名称、kind、面积、流水级数、有效 timing、timing 来源以及是否带 characterization metadata。`timing_source == "named"` 表示该 cell 使用特征化或显式命名 timing；`"kind"` 表示回退到 kind 级默认 timing；`"corner_named"` / `"corner_kind"` 表示该 timing 来自当前 active timing corner 的命名覆盖或 kind 覆盖；`"missing"` 表示该 cell 当前没有可用 timing，正式接入前必须补齐或解释。

PDK timing corner 采用 overlay 方式建模：基础 `cell_timing`、`named_cell_timing` 和 `interconnect_timing` 仍然是默认 corner；`timing_corners[]` 可以按 corner 名称补充 process、voltage、temperature 元数据和局部 timing 覆盖；`active_timing_corner` 选择当前用于 STA / report 的 corner。查找顺序为 active corner named timing、基础 named timing、active corner kind timing、基础 kind timing；interconnect timing 则优先使用 active corner 的同类 model，再回退基础 model。缺失的 corner 条目会回退基础 PDK，因此旧 PDK 和只覆盖少量慢速路径的 corner 都能保持兼容。

命令行也提供同等的机器可读索引入口：

```bash
cargo run -p rflux-cli -- pdk-cell-library --input target/minimal_pdk.json
cargo run -p rflux-cli -- pdk-cell-library --input target/minimal_pdk.json --kind macro
cargo run -p rflux-cli -- pdk-cell-library --input target/minimal_pdk.json --cell sfq_gate
```

`pdk-cell-library` 报告会包含稳定的 `library` 区块（`artifact_kind == "rflux_cell_library"`、`name`、`version`、`source`、`schema`、`capabilities`、`coverage`）、兼容性根字段 `cell_library_*`、`active_timing_corner`、`timing_corners` 和 `remediation` 区块。接入脚本应优先消费 `library.schema.name == "rflux_cell_library_manifest"` 且 `library.schema.version == 1` 的报告；`library.capabilities` 声明该报告支持按 cell 名称、kind 查询，并报告有效 timing、characterization metadata 与 remediation；`library.coverage` 汇总 cell/kind/timing/characterization 覆盖状态。若 `remediation.timing.status == "action_required"`，优先查看 `remediation.timing.cells`，并为这些 cell 补 `named_cell_timing` 或补齐其 `SfCellKind` 的 kind-level `cell_timing`。`remediation.characterization.status == "advisory"` 表示结构可用，但仍建议对高价值 macro / compound cell 增加 characterization metadata 与 arc delays。

`library.schema.version` 是 cell library manifest 的机器合同版本；只要该版本保持为 `1`，`library.artifact_kind`、`library.schema`、`library.capabilities`、`library.coverage` 的字段语义应保持向后兼容。若将来需要破坏性调整，应提升 manifest schema 版本，而不是静默改变这些字段。

### 4.3 验证阶段

至少执行以下验证：

- PDK schema / 结构校验：优先执行 `cargo run -p rflux-cli -- pdk-validate --input <path-to-pdk.json>`。
- 最小 CLI smoke：`pdk-minimal` 类路径或等价加载测试。
- compile / layout / timing 基线回归。
- 如果涉及仿真或 characterization，执行对应子流程回归。

当前 `pdk-validate` 直接命令已有显式 CI smoke 锚点，而不只是隐含地落在 `cargo test --workspace` 里：

- `cargo test -p rflux-cli run_pdk_validate_reports_ -- --nocapture`

当前说明：

- `pdk-validate` 当前会自动检查以下最小不变量：
- PDK 名称非空、`metal_layers > 0`。
- `cell_library` 中没有空名、重复名或负面积 cell。
- `cell_library` 至少覆盖 `GenericGate`、`Macro`、`Splitter`、`Dff`、`Jtl`、`Ptl`、`Port` 七类基础 `SfCellKind`。
- `cell_timing` 没有重复 kind、负 delay 参数，并且对上述七类 `SfCellKind` 都有默认 timing 覆盖。
- `named_cell_timing` 不引用悬空 cell、没有重复 cell 项、没有负 delay 参数，且 named timing 的 kind 与 cell library 一致。
- `active_timing_corner` 必须匹配 `timing_corners[]` 中的某个 corner；每个 timing corner 名称非空且不重复，corner 内的 `cell_timing`、`named_cell_timing`、`interconnect_timing` 也会执行重复项、负值、点序和 cell kind 一致性检查。
- `characterized_cell_metadata` 不引用悬空 cell、没有重复 cell 项，且 characterization 的 `delay_calibration_sigma_ps`、delay detail、arc delay 等数值不为负。
- characterization arc 不能出现重复的 `(driver_cell_name, from_port, sink_cell_name, to_port)` 签名，避免 timing 在查找 arc delay 时退化为顺序依赖。
- `ptl_forbidden_ranges` 不出现负值或倒置区间。
- `interconnect_timing` 没有重复 model、空点集、负值点或非严格递增点序，并至少覆盖 `Jtl` / `Ptl` 两类基础 interconnect model。
- `pdk-validate` 报告现在包含 `summary` 与 `checks`，用于快速查看 cell/timing/interconnect/timing corner/characterization 覆盖数量，以及 required cell kind、required timing、required interconnect timing、timing corners、named timing、characterized arcs、PTL forbidden ranges 等分项状态。
- `pdk-validate` 的 `summary.cell_library_*` 与 `checks.cell_library_index` 会复用 `pdk-cell-library` 的索引口径，直接展示库名、可用 kind、kind_counts、named/kind/missing timing 数量、对应 cell 名单、已带 characterization metadata 的 cell 数量和 remediation 建议。
- `pdk-validate` 也会输出 advisory `warnings`，例如 characterization arc 指向未知 driver/sink cell 或 metadata 缺少 arc delay；这类提示不一定阻止 PDK 加载，但应在正式评估前归因。
- `pdk-validate` 通过不代表该 PDK 已达到正式支持；它只是接入流程中的第一道自动检查。

### 4.4 发布阶段

- 更新支持矩阵。
- 更新已知限制。
- 更新 PDK 版本说明。
- 为该 PDK 指定 benchmark 集与负责人。

## 5. 必须记录的元数据

- PDK 名称。
- PDK 版本。
- schema 版本。
- 数据来源。
- 负责人。
- 首次接入日期。
- 当前支持等级。
- 已知限制列表。

## 6. 不允许的接入方式

以下做法不允许作为正式接入：

- 直接把临时 JSON 丢进仓库而无验证记录。
- 没有 benchmark 回归就宣称支持新 PDK。
- 用隐式默认值掩盖缺失字段。
- 没有负责人和来源记录的工艺数据。

## 7. 当前建议

在商业化早期，优先策略应为：

- 把 `minimal-sfq` 维护成最稳定基线。
- 先把一套真实 PDK 接到可重复、可验证。
- 不要同时推进多套工艺产品化承诺。
