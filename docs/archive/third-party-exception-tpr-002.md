# rflux 第三方组件例外与人工补审记录 TPR-002 v0

## 1. 基本信息

- 记录 ID：`TPR-002-A1`
- 组件名称：`colorama`
- 组件类型：Python 依赖
- 当前版本：`0.4.6`
- 风险级别：R2
- 对应台账项：`TPR-002`

## 2. 触发原因

- 触发时间：2026-05-28
- 触发事件：Python 许可证导出审查
- 问题摘要：当前 Python 许可证 inventory 中，`colorama 0.4.6` 没有 `License-Expression` 和 `License` 字段，仅提供 `BSD License` classifier 作为许可线索；这不阻止自动导出继续运行，但需要人工确认该线索是否足够满足当前使用边界。

## 3. 当前证据

- 自动化输出：`uv run python python/scripts/export_python_license_inventory.py --output %TEMP%\\rflux-python-license-inventory.json` 的当前 inventory 显示 `colorama` 的 `license_expression = null`、`license = null`、`license_classifiers = ["License :: OSI Approved :: BSD License"]`，`status = ok`。
- 人工核对来源：`docs/third-party-risk-register.md`、`docs/third-party-risk-review.md`、`python/scripts/export_python_license_inventory.py`。
- 当前已知限制：仓库当前仅做 wheel 元数据优先导出，尚未对所有包统一要求 SPDX expression；因此 classifier-only 条目需要人工确认是否可接受。

## 4. 例外或补审结论

- 结论类型：人工补审通过
- 结论摘要：`colorama 0.4.6` 当前仅凭 BSD classifier 作为许可线索，暂可接受继续使用；该条目应继续保留在后续许可证审查中复核。
- 当前可接受范围：
  - 允许保留当前依赖版本
  - 允许在许可证 inventory 中以 classifier-only 形式继续跟踪
- 不可接受范围：
  - 在没有进一步证据时把 classifier-only 条目表述成完整 SPDX 许可表达
  - 将该条目作为面向外部再分发的唯一许可依据

## 5. 审批与责任

- DRI：CLI / Python / 产品化负责人
- 必要评审方：QA / 基准负责人、文档与支持负责人
- 审批时间：2026-05-28
- 有效期或复审时间：当 `colorama` 升级，或 Python 许可证导出策略升级为强制 SPDX 级别时复审

## 6. 后续动作

- 关闭条件：为 `colorama` 补充更明确的 SPDX 级别许可证来源，或确认 classifier-only 线索已满足当前分发边界。
- 跟踪动作：
  - 继续观察下次 Python 许可证导出是否出现新的缺失项
  - 若后续引入更严格的许可证策略，再把该条目升级为显式 SPDX 证据
- 相关文档更新：`docs/third-party-risk-register.md`、`docs/security-compliance.md`、`python/scripts/export_python_license_inventory.py`
