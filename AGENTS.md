# AGENTS.md — rflux 仓库协作说明

面向 AI 助手与贡献者。Rust 为核心实现语言；Python 用于绑定、脚本与与 ColdFlux 等生态的胶水层。

---

## Python 与 uv（必须遵守）

本仓库**所有 Python 依赖与虚拟环境均通过 [uv](https://docs.astral.sh/uv/) 管理**。不要使用 `pip install`（全局或裸用）、`poetry`、`pipenv` 或手工维护 `requirements.txt` 作为主依赖来源。

### 约定

| 项 | 规则 |
|----|------|
| 项目根配置 | `pyproject.toml`（`[project]`、`[tool.uv]`、可选 `[tool.uv.workspace]`） |
| Python 版本 | 根目录 `.python-version`（与 `requires-python` 一致，当前建议 **3.12**） |
| 锁文件 | 提交 `uv.lock`；改依赖后运行 `uv lock` |
| 虚拟环境 | 仓库根 `.venv/`（由 `uv sync` 创建；加入 `.gitignore` 若未忽略） |
| 包布局 | 可安装包在 `python/rflux/`；绑定构建见 `crates/py/` |

### 常用命令

```bash
# 安装/同步全部依赖（含 dev）
uv sync

# 运行工具（自动使用项目 venv）
uv run pytest
uv run python -c "import rflux"

# 添加依赖
uv add numpy          # 运行时
uv add --dev pytest   # 开发

# 仅更新锁文件
uv lock

# 构建并安装本地 PyO3 扩展 + python/rflux（混合包，开发模式）
uv run maturin develop
```

### 禁止与例外

- **禁止**：`pip install -r requirements.txt`、`python -m pip install <pkg>`（除非在文档中明确的一次性调试，且不得写入仓库规范）。
- **禁止**：在未更新 `pyproject.toml` / `uv.lock` 的情况下向 CI 或文档推荐新 Python 包。
- **例外**：仅当 uv 无法覆盖的第三方工具明确要求系统 Python 时，在 issue/文档中说明；默认仍优先 `uv tool install` 或 `uv run --with`。

### 测试与脚本

- 测试：`uv run pytest`（配置在 `pyproject.toml` 的 `[tool.pytest.ini_options]`）。
- 一次性脚本：放在 `python/scripts/`，首行注释说明用途；可执行入口用 `uv run python python/scripts/...`。
- Notebook：放在 `python/notebooks/`，内核使用本项目 `.venv`（`uv sync` 后选择该解释器）。

---

## Rust 与 Python 分工

| 层级 | 技术 | 说明 |
|------|------|------|
| 核心算法、IR、P&R、STA | Rust (`crates/*`, `rflux-*`) | 性能与类型安全 |
| 对外 Python API | PyO3 + maturin (`crates/py` → `rflux`) | 暴露稳定子集：IR 读写、报告、驱动 CLI |
| 胶水 / 批处理 / 可视化 | Python (`python/rflux`) | 对接 ColdFlux、JoSIM 批跑、基准对比 |

新增功能时：**默认用 Rust 实现**；仅当需要 NumPy/Notebook/现有 Python EDA 脚本集成时，在 Python 层做薄封装。

---

## 修改 Python 绑定时的检查清单

1. 若改动 `crates/py` 的 Rust API，同步更新 `python/rflux` 的类型存根或文档字符串。
2. 运行 `uv run maturin develop -m crates/py/Cargo.toml`（或 CI 等价命令）确认扩展可导入。
3. 运行 `uv run pytest`。
4. 不在 Python 中复制核心业务逻辑（应调用 Rust 扩展或 CLI）。

---

## 文档

- 架构与 Python 绑定设计：[docs/project-design.md](docs/project-design.md)（「Python 绑定」一节）
- SFQ 领域背景：[docs/sfq.md](docs/sfq.md)
