# rflux 外部命令调用策略 v0

## 1. 目的

本文件定义 `rflux` 当前允许的外部命令调用边界、调用方式和变更规则。

它的目标不是开放通用插件执行能力，而是把现阶段唯一保留的外部仿真路径固定成可审查、可测试、可文档化的最小策略。

## 2. 当前允许项

当前仓库只允许以下外部命令：

- `josim`
- `josim.exe`
- `josim.cmd`
- `josim.bat`
- `josim.sh`
- `josim-cli`
- `josim-cli.exe`
- `josim-cli.cmd`
- `josim-cli.bat`
- `josim-cli.sh`

说明：

- 允许裸命令 token，也允许路径形式，只要最终文件名在去掉受限 wrapper 后缀（`.exe` / `.cmd` / `.bat` / `.sh`）后仍是 `josim` 或 `josim-cli`；其他程序名仍会被拒绝。
- 当前策略与 `rflux-sim` 中的 allowlist 实现一致。

## 3. 当前调用方式

当前外部命令只出现在仿真路径：

- 入口：`simulate_text(...)` / `simulate_file(...)` 对应的外部仿真模式。
- 调用条件：调用方显式提供 `external_command`，并选择 `external_josim` 或 `auto` 路径。
- 调用形式：`std::process::Command::new(command).arg(deck_path)`。
- 当前不通过 shell 拼接命令行。

## 4. 当前安全边界

当前策略已经具备以下最小约束：

- 最小 allowlist：只允许 `josim` / `josim-cli` 及其受限 wrapper 后缀 `.exe` / `.cmd` / `.bat` / `.sh`。
- 最小路径信任规则：允许路径形式，但只按最终可执行文件名做 JoSIM allowlist 匹配，不放开任意其他程序。
- 最小输入边界：外部程序当前只接收生成的 deck 文件路径一个参数。
- 最小环境隔离：当前会在外部子进程中移除 `RFLOW_*` 与 `JOSIM_*` 环境变量，避免仓库自身控制变量直接泄漏到外部仿真器。
- 最小副作用文件约束：每次外部运行都会先在系统临时目录下创建独立的 `rflux-ext-*` 子目录，并把输入 deck 写入该目录，降低默认输出文件污染其他路径的概率。

以下能力仍未具备：

- sandbox
- 签名校验
- 更完整的环境变量 allowlist / denylist 策略
- 二进制来源证明或路径 attestation
- 更明确的外部输出文件 allowlist / 生命周期管理
- 面向多外部仿真器的策略扩展机制

## 5. 变更规则

任何新增或放宽外部命令策略的改动，至少必须同时满足：

- 已更新本文件。
- 已更新 `docs/security-compliance.md`。
- 已补充回归测试，覆盖允许与拒绝路径。
- 已说明新增外部工具的来源、用途和支持边界。
- 已确认不会把任意程序执行暴露成公共接口默认行为。

## 6. 当前建议

在进入更高等级发布前，应继续补齐：

- 可审查的策略配置载体。
- 更明确的路径信任与来源约束。
- 外部执行相关日志与诊断记录。
- 对副作用文件生命周期和失败模式的正式约束。
