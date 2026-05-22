# 诊断包基线

当前仓库已提供最小 CLI 诊断包导出能力，用于把单次问题复现所需的核心上下文收集到一个可归档目录中。

## 命令

```bash
cargo run -p rflux-cli -- collect-diagnostics \
  --output-dir target/diagnostics/example \
  --command simulate-file \
  --input path/to/example.ir.json \
  --pdk path/to/example.pdk.json \
  --report target/reports/simulate-report.json \
  --mode internal_transient \
  --external-command josim \
  --notes "capture for support reproduction"
```

## 当前输出内容

- `manifest.json`：诊断包元数据、平台信息、CLI 版本、调用参数、当前工作目录、最小环境摘要。
- `events.jsonl`：诊断包采集过程的结构化事件日志，当前覆盖 bundle 开始、输入复制和 manifest 生成。
- `inputs/`：按原文件名复制的输入文件副本，当前支持 `--input` 和 `--pdk`。
- `reports/`：按原文件名复制的现有 JSON report，当前支持 `--report`。

`manifest.json` 中当前还会附带：

- `configuration`：标准化路径回显与仿真相关配置回显。
- `summary`：采集到的输入数量、legacy 兼容输入、契约检查失败统计。
- `captured_reports`：现有业务 JSON report 的复制结果，以及 `kind` / `schema_version` / 解析错误摘要。
- `structured_logs`：当前结构化日志文件路径、格式和事件数。
- `RFLOW_*` / `JOSIM_*` 已出现环境变量名清单（仅记录名称，不记录值）。
- `--input` / `--pdk` 的契约快照：versioned envelope / legacy raw JSON 路径，以及 schema version。
- JSON 检查失败时的 `inspection_error`，避免因为诊断包采集而丢失原始坏输入。

## 当前用途

- 让支持和研发拿到一份固定目录，避免只靠口头描述问题。
- 为后续结构化日志、运行摘要和性能 profile 开关预留统一归档位置。

## 当前限制

- 还未自动打包实际业务命令的实时运行日志；当前只能把已有 JSON report 作为事后产物一并收进诊断包。
- 还未导出完整配置快照或全量依赖环境；当前只覆盖路径/仿真参数回显、运行摘要、输入契约快照和已有 report 摘要。
- 还未包含性能 profile、外部工具版本探测或压缩归档封装。

因此，当前诊断包应视为“最小可复现上下文基线”，不是完整商业支持包。