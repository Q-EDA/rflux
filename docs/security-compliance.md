# rflux 安全与合规基线

## 1. 目的

本文件记录 `rflux` 当前已经落地的安全与合规基线、自动化执行面和已知缺口。

它不是最终的企业级安全手册，而是把路线图中“依赖清单、许可证清单、基础安全扫描、外部命令边界”先收敛为可执行的最小制度。

## 2. 当前基线

### 2.1 许可证基线

- Rust workspace 当前统一声明双许可证：`MIT OR Apache-2.0`。
- 各 crate 通过 workspace 级 `license` 继承该声明。
- README 已向外部用户公开当前许可证模型。

### 2.2 当前自动化基线

当前仓库已开始提供一个可触发的合规作业，用于产出以下工件：

- Rust 依赖清单：`cargo metadata --format-version 1`
- Rust 许可证清单：`cargo license --json`
- Rust 安全扫描结果：`cargo audit`
- Python 依赖清单：`uv.lock` 导出的机器可读 inventory
- Python 许可证清单：基于 `uv.lock` 中 wheel 元数据导出的机器可读 inventory
- Python 依赖审查输入：`pyproject.toml` 与 `uv.lock`

这些工件当前通过 GitHub Actions artifact 保留，供内部审查与发布前检查使用。

当前合规 artifact 生成已集中到 `python/scripts/prepare_security_compliance_artifacts.py`，由 optional workflow 统一产出 inventory、审查输入、副本文件和 manifest/README，而不再在 workflow 里散落多段 shell 重定向。

对外部仿真失败后保留的 `rflux-ext-*` 运行目录，当前默认保留 7 天用于复审；复审完成后应先用 `python/scripts/cleanup_external_run_artifacts.py` 做 dry-run，再在确认无须保留时加 `--delete` 清理过期目录。

### 2.3 外部命令调用边界

当前仓库存在外部命令执行面，主要在仿真路径：

- 只有当调用方显式提供 `external_command`，并选择 `external_josim` 或 `auto` 下的外部路径时，才会触发外部命令调用。
- 当前实现只允许最终文件名在去掉 `.exe` / `.cmd` / `.bat` / `.sh` 后匹配 `josim` / `josim-cli`；路径形式允许，但其他程序名仍会在进入 `Command::new(...)` 前被拒绝。
- 当前实现使用 `std::process::Command::new(command).arg(deck_path)` 调用外部程序，不通过 shell 拼接命令行。
- 当前已具备最小 allowlist、最小路径信任规则、最小环境隔离和最小副作用文件约束，且策略已固化在 `docs/external-command-policy.md`；但尚未建立 sandbox、签名校验或更完整的环境与文件策略，因此 `external_command` 仍应被视为受信任操作者输入，而不是面向不受控用户输入的安全接口。

## 3. 当前已知缺口

以下能力仍未完成，不应误表述为已产品化：

- 未形成正式 SBOM 发布制度；当前仅有依赖/许可证/扫描工件基线。
- Python 许可证清单当前依赖 wheel 元数据；当前导出会优先读取本地 `uv` 缓存，并显式统计缺少 wheel、缺少许可证元数据或抓取失败的包；在 `UV_OFFLINE=1` 下未命中的项会直接记为抓取失败而不是尝试联网。这些项仍需按 `docs/third-party-exception-template.md` 做人工补审。
- 外部命令调用尚无 sandbox、签名校验或完整环境/文件策略；当前已有最小 JoSIM allowlist、最小路径信任规则、仓库控制变量隔离、独立临时运行目录和策略文件。
- 安全扫描尚未成为 GA 级发布硬门，仅作为当前基线作业。
- 第三方组件风险审查清单、初始台账和补审模板已建立，但正式的例外审批流程和长期跟踪机制仍未完成。

## 4. 当前执行方式

建议在以下时点运行合规作业：

1. 每个内部候选版本出包前。
2. 引入新第三方依赖后。
3. 调整外部命令调用路径后。
4. 准备对外试点或正式发布前。

Python 合规导出命令族现在也有显式 CI smoke anchor，而不只是依赖全量 `uv run pytest`：

- `uv run pytest python/tests/test_security_compliance_utils.py -q`

## 5. 达到阶段 4 要补齐的内容

要满足路线图中“10.3.4 安全与合规”的要求，至少还需要补齐：

- SBOM 产物格式与发布路径。
- Python 许可证缺失项的人工补审流程。
- 第三方组件风险例外审批流程与长期跟踪机制。
- 外部命令调用 sandbox、签名校验与更完整的环境/文件策略。
- 将安全与许可证检查纳入正式发布准入门。
