# rflux 第三方组件风险台账 v0

## 1. 目的

本文件用于记录 `rflux` 当前第三方组件的风险结论、例外状态和后续动作。

它是 `docs/third-party-risk-review.md` 的执行台账，不替代规则文件，而是承接实际审查结果。

## 2. 使用规则

- 每个 `R3` 级组件必须至少有一条记录。
- 任何仍需人工补审或例外处理的项目，必须在“例外状态”与“后续动作”列明确写出。
- 当风险状态变化时，应更新本文件，而不是只改路线图或 README。
- 具体审批或补审记录应使用 `docs/third-party-exception-template.md`。

## 3. 当前台账

| 组件 | 类型 | 风险级别 | 当前用途 | 当前结论 | 例外状态 | 负责人角色 | 证据 |
|------|------|----------|----------|----------|----------|------------|------|
| `josim` / `josim-cli` 及受限 wrapper 后缀 | 外部命令工具 | R3 | 受限外部仿真路径 | 当前通过 `rflux-sim` allowlist 调用，只允许最终文件名在去掉 `.exe` / `.cmd` / `.bat` / `.sh` 后匹配 JoSIM 已知名称；不通过 shell 拼接，会移除 `RFLOW_*` / `JOSIM_*` 子进程环境变量，并把单次运行输入写入独立 `rflux-ext-*` 临时目录；成功运行后会将 deck / waveform 复制到稳定临时文件并清理运行目录，失败运行会保留运行目录以便复审 | 开放例外：尚无 sandbox、签名校验、完整环境隔离与更明确的外部输出保留策略 | 核心内核负责人 + CLI / Python / 产品化负责人 | `docs/external-command-policy.md`、`docs/third-party-exception-tpr-001.md`、`crates/sim/src/lib.rs`、对应回归测试 |
| Python wheel 元数据许可证导出 | Python 依赖审查机制 | R2 | Python 许可证 inventory 自动化 | 当前优先读取本地 `uv` 缓存中的 wheel `METADATA`，缓存缺失时再回退到 wheel URL，并显式统计缺失项与抓取失败项 | 开放例外：缺少 wheel、缺少许可证元数据，或元数据抓取失败的包仍需人工补审 | CLI / Python / 产品化负责人 + QA / 基准负责人 | `python/scripts/export_python_license_inventory.py`、`python/tests/test_security_compliance_utils.py` |

## 4. 开放例外台账

| ID | 主题 | 当前影响 | 临时接受理由 | 关闭条件 |
|----|------|----------|--------------|----------|
| TPR-001 | `josim` 外部执行缺少 sandbox / 签名校验 / 完整环境隔离 | 外部仿真路径仍属于受信任操作者接口，不能当作不受控输入面 | 当前已具备最小 allowlist、最小路径信任规则、仓库控制变量隔离、独立临时运行目录，以及成功后输出副本与运行目录清理；失败运行会保留目录以供复审；首条例外记录见 `docs/third-party-exception-tpr-001.md` | 形成外部执行策略配置、完整环境策略和更明确的来源验证 |
| TPR-002 | Python 许可证导出存在元数据缺失或抓取失败场景 | 某些包可能只能得到部分许可证信息，或在当前网络/缓存条件下无法完成抓取，不能自动闭环 | wheel 元数据已经足以覆盖当前已锁定依赖中的一部分关键包，比已安装环境元数据更可靠；当前导出已能显式列出缺失项与抓取失败项 | 使用 `docs/third-party-exception-template.md` 形成缺失项补审记录，必要时增加 sdist/registry 级回退路径 |
| TPR-002-A1 | `colorama 0.4.6` classifier-only 许可线索 | Python 许可证 inventory 里 `License-Expression` / `License` 缺失，只能依赖 `BSD License` classifier 进行人工确认 | 当前 classifier 线索与上游 wheel 元数据一致，且不影响当前自动导出；人工补审已接受继续使用 | 当 `colorama` 升级或许可证策略收紧时复审 |

## 5. 下次更新优先级

1. 给 `TPR-001` 继续补齐外部输出保留策略的最终归档规则（例如何时手动清理失败目录）。
2. 继续观察 `TPR-002-A1` 在后续导出里的稳定性，并在策略升级时复审。
3. 随真实发布依赖变化补充更多 `R2` / `R3` 组件记录。
