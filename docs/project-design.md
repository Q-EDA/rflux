# Rust-SFQ EDA 工具链项目方案

## 项目名称

**rflux**  
*子标题: Rust-based EDA Toolkit for Superconducting SFQ Circuits*

备选名称: `Qs-EDA` (Quantum Single-flux quantum), `ColdCargo`, `Fluxforge`

---

## 项目目标

构建一套以 **Rust 为核心**、辅以 **Python 绑定与脚本层** 的模块化工具链，用于支持超导 SFQ（单磁通量子）数字电路的 **设计、综合、物理实现、时序分析与验证**。项目旨在解决传统 EDA 工具（基于 C++/Python）难以处理的 SFQ 独特约束（脉冲信号、路径平衡、JTL/PTL 混合布线、交流偏置等），并利用 Rust 的内存安全、零成本抽象和强大宏系统打造高性能内核，同时通过 **PyO3 + uv** 提供与 ColdFlux/qPALACE 等 Python 生态的互操作能力。

### 核心目标
1. **提供 SFQ 专用中间表示 (IR)** – 支持脉冲级数据流和时钟约束的显式表达。
2. **实现 SFQ 逻辑综合流程** – 扇出分裂自动插入、路径平衡触发器插入、复合单元映射。
3. **实现时序驱动的布局布线** – 支持 levelized placement、JTL/PTL 混合分层路由、保持时间违例修复。
4. **提供分层验证仿真能力** – 混合 JoSIM/事件驱动仿真、统计静态时序分析 (SSTA)。
5. **建立可扩展器件库接口** – 支持 0-JJ/π-JJ、多层堆叠、交流/直流偏置模型。
6. **提供 Python 绑定与脚本接口** – 通过 PyO3 暴露 IR/报告/驱动能力，便于与现有 Python EDA 流程（如 ColdFlux）集成；Python 依赖统一由 **uv** 管理（见根目录 `AGENTS.md`）。

### 非目标（初期）
- 完整替代 ColdFlux/qPALACE 全部功能。
- 提供 GUI 图形界面（优先 CLI）。
- 支持室温 CMOS 设计。

### 实现约束
1. **项目核心保持纯 Rust 实现**：核心 crate 默认不得依赖需要额外 C/C++/系统工具链参与链接的第三方 crate。
2. **优先保证 WebAssembly 可编译性**：核心算法与数据结构需以 `wasm32-unknown-unknown` 为可支持目标进行设计，避免把平台绑定到 native-only 依赖上。
3. **外部依赖选择以可移植性为先**：若某类能力只能通过带外部 C 依赖的 crate 获得，优先在仓库内实现最小纯 Rust 版本，而不是引入破坏 wasm 构建链的依赖。

### SFQ 约束驱动设计原则
1. **时钟与数据流协同建模**：SFQ 门级网络不是“组合逻辑 + 全局时钟”的 CMOS 变体，而是带显式时钟输入、脉冲传播窗口和路径平衡约束的同步流水数据流。
2. **扇出受限优先于逻辑复用**：每个输出默认只允许单一消费，扇出必须通过显式 `Splitter` 树实现，并在 IR 和综合阶段持续保持该约束。
3. **宏单元分层优先**：对高频关键路径优先采用多级复合单元/宏单元，将门级时钟接入收敛到宏边界，减少 DFF 深度与布线复杂度。
4. **物理设计即电气设计**：JTL/PTL 选择、PTL 长度限制、时钟相位、偏置网络和材料堆叠约束必须共同参与布局布线与 STA，而不是在后仿真阶段补救。
5. **分层混合验证**：小规模关键子电路保留 JoSIM/SPICE 精度，大规模网络采用事件驱动脉冲仿真与时序抽象模型，避免全电路模拟仿真失控。

### 与 CMOS EDA 的本质差异

SFQ EDA 不是对传统工具的“功能增补”，而是在**逻辑表示、时序模型、物理设计、验证哲学**多个层面需要重新设计。下表概括各阶段的核心差异及 `rflux` 的应对方向（参考 qPALACE v2 / ColdFlux 开源实践）：

| 设计阶段 | CMOS EDA 常见假设 | SFQ 独特挑战 | rflux 应对 |
|----------|-------------------|--------------|------------|
| 逻辑综合 | 门级映射 + 全局时钟 | 每门独立时钟；扇出极弱；destructive readout 强制路径平衡 | 数据流-时钟紧耦合综合；自动插入 Splitter 树与平衡 DFF；支持多级复合单元 |
| 物理设计 | 放置-绕线，多层金属 | 电感占面积大；布线层极少；偏置电流网络复杂 | JTL/PTL 混合分层路由；FPL 堆叠 JJ 减面积；AC/DC 偏置网络建模 |
| 时序分析 | RC 延迟，建立/保持 | 脉冲到达窗口；PTL 反射；\(J_c\) 工艺偏差；偏置网络寄生 | 脉冲窗口 STA + SSTA；PTL 危险长度区间；违例修复接口 |
| 仿真验证 | 事件驱动逻辑仿真 | 全电路 SPICE 不可扩展；PTL 反射/串扰需电气检查 | 分层混合仿真（JoSIM + 脉冲事件驱动） |
| 器件/材料 | 标准 MOSFET 模型 | 0-JJ / π-JJ、多层超导堆叠、制造约束 | `rflux-device` + `rflux-tech` 显式建模 |

### 参考项目与定位

| 项目 | 角色 | 对 rflux 的启示 |
|------|------|-----------------|
| **ColdFlux / qPALACE v2** | 开源 SFQ 全流程（综合、仿真、P&R、功耗） | 宏单元分层、时序驱动 P&R、混合仿真策略的直接参考；深度绑定 MIT-LL 等特定 PDK，跨工艺可移植性有限 |
| **RustSFQ** | Rust 嵌入式 SFQ-HDL | 所有权模型在编译期保证“单输出单消费”，可融入 `rflux-ir` / `rflux-hdl` |
| **JoSIM** | SPICE 级 JJ 电路仿真 | `rflux-sim` 底层精确验证引擎（子进程或 FFI） |

**项目定位**：不追求一期替代 ColdFlux 全功能，而是提供 **Rust 模块化、可嵌入** 的 SFQ 专用核心（IR、综合、脉冲 STA、JTL/PTL 路由、混合仿真），并支持与现有流程通过文件格式或 FFI 集成。

### 为何采用 IR 优先而非 Verilog 原生语义

即使 qPALACE 等工具以 Verilog 为前端并最终生成 GDS-II，将 SFQ 电路强行映射到 Verilog 仍存在两个结构性问题：

1. **Verilog 基于电平逻辑与全局时钟**，而 SFQ 是**事件/脉冲驱动**逻辑；电平→脉冲的隐式转换依赖设计者与库建模，脆弱且易错。
2. **传统 CMOS 综合工具无法直接复用**；必须为 SFQ 定制整条工具链。

因此 `rflux` 以**脉冲级 IR** 为主设计载体，Verilog 仅作为可选输入前端之一；`rflux-hdl` 提供更贴近 SFQ 的 Rust DSL（借鉴 RustSFQ、cmtrs、fayalite 思路）。

---

## 设计文档

### 一、总体架构

```
rflux (workspace)
├── crates/                  # 目录名不含 rflux 前缀；Cargo 包名统一为 rflux-*
│   ├── ir/                  -- rflux-ir: SFQ 中间表示 (IR)
│   ├── device/              -- rflux-device: 器件模型 (JJ, JTL, PTL)
│   ├── tech/                -- rflux-tech: 工艺技术库 (层叠、设计规则)
│   ├── hdl/                 -- rflux-hdl: Rust DSL / 脉冲级建模前端
│   ├── synth/               -- rflux-synth: 逻辑综合
│   ├── flow/                -- rflux-flow: 端到端编排 (synth -> place -> route)
│   ├── place/               -- rflux-place: 布局 (placement)
│   ├── route/               -- rflux-route: 布线 (JTL/PTL routing)
│   ├── timing/              -- rflux-timing: 时序分析与 STA
│   ├── sim/                 -- rflux-sim: 仿真引擎 (事件驱动 + JoSIM 胶水)
│   ├── verify/              -- rflux-verify: 形式验证 / 等价性检查
│   ├── io/                  -- rflux-io: 文件解析 (Verilog, LEF/DEF, GDS, SPICE)
│   ├── cli/                 -- rflux-cli: 命令行库（供 bins 链接）
│   └── py/                  -- rflux-py: PyO3 扩展（maturin 构建 → import rflux）
├── python/
│   ├── rflux/               -- 纯 Python 包（薄封装、类型存根、高层 API）
│   ├── scripts/             -- 批处理与基准脚本（uv run）
│   └── notebooks/           -- 探索性分析与可视化
├── pyproject.toml           -- Python 项目与 uv 配置
├── uv.lock                  -- 锁文件（须提交）
└── bins/
    ├── rfluxc               -- 编译器主驱动
    ├── rflux_place          -- 布局工具
    ├── rflux_route          -- 布线工具
    ├── rflux_sta            -- 时序分析
    └── rflux_sim            -- 仿真运行器
```

**命名约定**：`crates/<name>/` 为源码目录；`Cargo.toml` 中 `name = "rflux-<name>"`（如 `crates/ir` → `rflux-ir`）。依赖声明与 `use` 路径使用 crate 名（`rflux_ir`），不与目录名混用。

**Python 约定**：可安装发行名为 `rflux`（`import rflux`）；实现体为 `crates/py` 的本地扩展 + `python/rflux` 的纯 Python 代码。依赖与 venv **仅通过 uv** 管理，细则见仓库根目录 [AGENTS.md](../AGENTS.md)。

### 二、核心模块设计

#### 2.1 `rflux-ir` – SFQ 中间表示

- **设计要点**：Rust 枚举 + 图结构，表示脉冲驱动的数据流图，并显式编码时钟、相位和脉冲窗口。
- **基础节点**：
  - `CellInstance` (标准单元实例, 带时钟输入)
  - `MacroCell` (多级复合单元/预制宏块, 仅边界接收时钟)
  - `Splitter` (分裂器, 显式扇出)
  - `DFF` (延迟触发器, 含路径平衡标记)
  - `JTL` (传输线段, 指定长度)
  - `PTL` (无源传输线, 指定长度)
  - `Port` (输入/输出)
- **属性**：时钟域、偏置类型 (AC/DC)、相位约束、脉冲有效窗口、宏边界约束。
- **所有权约束**：利用 Rust 类型系统保证每个输出信号只被一个输入消费（**RustSFQ 核心思想**——SFQ 中无显式 Splitter 则无法合法扇出；所有权可在**编译期**保证，传统流程依赖人工检查）。需要扇出时强制插入 `Splitter` 节点；该能力可作为 `rflux-hdl` / `rflux-verify` 的差异化特性。
- **建模策略**：IR 作为主设计载体，Verilog 仅作为输入前端之一；对 SFQ 特有的 destructive readout、路径平衡和脉冲语义，优先在 IR 层直接表达，而不是依赖 RTL 语义推断。

#### 2.2 `rflux-device` – 器件库

- **Josephson Junction 模型**：实现 RCSJ 模型参数 (`Ic`, `Rn`, `Cj`)；扩展 `thevenin` 的 Modified Nodal Analysis 框架以支持 JJ（当前 thevenin 以 CMOS 为主）。
- **传输线模型**：JTL 每级延迟（ps/μm），PTL 特征阻抗与反射条件；分布参数超导传输线模型用于跨模块电气检查。
- **单元库抽象**：预特征化单元 (SportLib) + 可综合复合单元；标准库仅含基本门时流水深度大、DFF 开销高，需**多级复合单元**（仅输入/输出级接收时钟）与门级库混合使用。
- **FPL（Fast Phase Logic）**：用堆叠 JJ 替代大面积电感器，单元面积可降至传统 RSFQ 的约 1/10，需在库抽象中区分 RSFQ/FPL 单元族。
- **交流偏置模型**：AC/DC 转换器及设计空间量化（见下文 AC 偏置权衡）；JTL 链可直接接收 AC 电源，简化 4K 温区供电网络。
- **器件变体**：
  - **0-JJ**：开关用，要求高 \(J_c\)。
  - **π-JJ**：提供 π 相位偏移，需磁性材料。
- **材料与堆叠信息**：AlN 缓冲层、TaN 阻挡层、Mo 接触金属等多层堆叠；工艺临界电流 \(J_c\) 偏差可达数个百分点，驱动 SSTA 需求。
- **特征化目标**：除面积、功耗外，记录最大工作频率、偏置方式、对时钟窗口的敏感度与 PTL/JTL 接口约束。

**AC 偏置设计空间（EDA 需自动探索，非手工试错）**：

| 指标 | DC 偏置（示例） | AC 偏置（示例） | 说明 |
|------|----------------|----------------|------|
| 静态功耗 | ~358.4 μW | 0 | AC 消除静态功耗 |
| JJ 数量 | 768 | 1792 (~2.3×) | 面积与复杂度上升 |
| 芯片面积 | ~0.23 mm² | ~0.29 mm² (~+23%) | |
| 最大工作频率 | ~60 GHz | ~20 GHz | 频率-功耗-面积权衡 |

#### 2.3 `rflux-tech` – 工艺与 PDK 抽象

- **目标**：统一描述工艺层、最小线宽/间距、JJ 堆叠选项、偏置网络规则、PTL 可用层和时钟分布规则。
- **设计规则**：建模少布线层约束、宏单元边界端口朝向、AC 供电布线资源和 PTL 危险长度区间。
- **接口**：为 `rflux-place`、`rflux-route`、`rflux-timing` 提供一致的技术查询 API，避免各模块重复编码工艺知识。

#### 2.4 `rflux-hdl` – Rust DSL / 脉冲级建模前端

- **定位**：提供比 Verilog 更贴近 SFQ 语义的 Rust 内嵌 DSL，用于直接描述脉冲逻辑、路径平衡和显式时钟关系。
- **目标用户**：算法原型、宏单元建模、器件级实验与需要编译期扇出检查的设计场景。
- **输出**：生成 `rflux-ir` 或 SPICE 子网表，作为综合、仿真和器件特征化的共同入口。

#### 2.5 `rflux-synth` – 逻辑综合

综合在 SFQ 中不仅是“门级映射”，更是**数据流与时钟的紧耦合变换**：

| SFQ 约束 | 工具行为 |
|----------|----------|
| 同步流水线，每门独立时钟输入 | 综合时显式管理时钟到各门的传播路径，禁止假设全局时钟 |
| 扇出驱动极弱 | 自动插入 **Splitter 树**，综合阶段即考虑扇出上限 |
| Destructive readout / 路径平衡 | 自动插入**平衡触发器**，否则电路无法正确工作 |
| 标准库仅基本门导致流水过深 | 优先映射**多级复合单元**，时钟仅接宏边界 |

- **输入**：Verilog RTL（`rflux-io`）、`rflux-hdl` DSL，或直接构造的 `rflux-ir`。
- **流程**：
  1. 布尔逻辑优化（内建纯 Rust 逻辑网络重写与共享子表达式合并，为 wasm 目标保持无外部 C 依赖）。
  2. 技术映射到 SFQ 单元库 (AND/OR/XOR/DFF + 复合单元)，优先选择多级复合单元以降低流水深度。
  3. 扇出分裂器自动插入（每个输出扇出 > 1 时插入 splitter tree）。
  4. 路径平衡触发器插入（在 combinational path 末端加 DFF 使各路径延迟匹配）。
- **时钟建模**：综合阶段需显式维护门级时钟到达关系，不能依赖“后续 CTS 自动修复”的 CMOS 假设；支持**双时钟/多相位**方案评估（对 DFF 时钟相位匹配敏感）。
- **输出分层**：除门级 `rflux-ir` 外，支持输出宏单元级网表；宏单元设计类似早期 CMOS 门阵列，但须同时满足**信号流向**与**时钟分布**约束。
- **输出**：`rflux-ir` 格式的网表。

#### 2.6 `rflux-place` – 布局

物理设计在 SFQ 中不是单纯的“放置-绕线”，而是**双轨信号、极少布线层、JTL/PTL 分层**的电气-几何协同问题。

- **算法**：二次规划 + 层次化合法化；**levelized placement**（qLevelPlace 思路）：时序关键路径密集放置，非关键路径放宽区域约束。
- **特点**：
  - 为时钟树预留专用布线通道（SFQ 时钟须路由到**每一个**门单元）。
  - 支持宏单元 (macro) 预放置：宏内 JTL，宏间 PTL 接口；**宏单元分层设计（Macro-based hierarchical design）** 为 qPALACE v2 核心方法。
- **关键优化目标**：优先压缩关键路径跨度，减少长 PTL；后布线频率与面积高度依赖 JTL 长度控制（见基准指标）。
- **参考改进幅度**（qPALACE v1 → v2，C880 等基准）：后布线频率约 14.5 → 15.8 GHz；多种电路后布线面积约减 14–23%；长连线显著缩短——本质来自 SFQ 时序与布线长度建模改进。
- **接口**：实现 `libreda_pnr` 的 `PlacementProblem` trait；**JTL/PTL 混合路由与脉冲 STA 不在通用框架中预设**，须自行实现算法。

#### 2.7 `rflux-route` – 布线

- **混合路由策略**（非“多层金属绕线”的 CMOS 模型）：
  - **宏单元内**：JTL 路由（A* + 延迟约束）；可跨层但面积大、延迟可预测。
  - **宏单元间**：PTL 路由，严格长度限制（避开反射危险区间）；仅在特定层、面积小但需电气长度预估。
  - **路由选择算法**：在布线层资源约束下为每条信号路径选择 JTL vs PTL。
- **时钟树合成 (CTS)**：与 CMOS 不同，时钟须送达**每个**门单元；H-tree 或多级 Splitter tree；支持双相位/多相位时钟（对 DFF 相位匹配敏感）。
- **保持时间修复**：与 `rflux-timing` 协同，自动插入长度精确 JTL 段（约 2–4 ps 步长）。
- **AC 偏置布线**：JTL 链可作为 AC 电源载体，简化 4K 温区供电（与 L5 分析层联动）。
- **约束检查**：PTL 电气长度、反射区间、时钟相位、偏置可达性均为一等约束。

#### 2.8 `rflux-timing` – 时序分析

时序验证基于**脉冲到达时间窗口**，而非电平跳变沿；并需考虑偏置网络寄生电感对 DC 偏置稳定性的影响（远期对接电-热-力联合优化接口）。

- **静态时序分析 (STA)**：
  - 单元延迟表（依赖输入脉冲与时钟到达窗口）。
  - PTL 反射检测：控制 PTL 电气长度，避开引起反射的长度区间；布线工具须能预估电气长度。
  - 建立/保持时间检查（脉冲窗口重叠）。
- **保持时间违例修复（SFQ 特色）**：在数据路径或时钟路径插入**长度精确的 JTL 段**微调时序（步长约 2–4 ps），无需 CMOS 式缓冲器链；需 **JTL 插入优化器**：最小化插入 JTL 总数并满足全部约束。
- **统计时序分析 (SSTA)**：临界电流 \(J_c\) 工艺偏差（数个百分点量级）；支持蒙特卡洛或解析矩方法。
- **分析对象**：以脉冲到达窗口为核心，显式检查 destructive readout 带来的路径同步要求。
- **修复接口**：向 `rflux-route` 和 `rflux-synth` 反馈可执行动作（JTL 微调、平衡 DFF、宏单元替换）。
- **接口**：输出时序报告 (JSON/YAML)，驱动 `rflux-route` 时序驱动迭代。

#### 2.9 `rflux-sim` – 仿真

全电路 SPICE/JSIM 仿真需亚皮秒级时间步长，规模扩大后不可行（例：c432 上 JoSIM ~614 s vs 抽象时序工具 ~24.3 s）。必须采用**基于时序模型的抽象仿真 + 局部精确仿真**。

| 层次 | 方法 | 原因 |
|------|------|------|
| ≤3 级深度子电路 | JoSIM / SPICE（`thevenin` 扩展或子进程） | 精确延迟与波形 |
| >3 级逻辑 | 事件驱动**脉冲**仿真器（Rust） | 规模与速度 |
| 跨宏单元 PTL 互连 | JoSIM 电气检查 | 反射、串扰无法用抽象模型完整捕捉 |

- **混合仿真引擎**：底层子电路 JoSIM；上层脉冲事件驱动；`thevenin` 扩展 RCSJ JJ 与超导传输线分布参数，用于特征化与局部分析。
- **加速技术**：增量编译、并行子电路仿真。
- **波形输出**：VCD 或自定义格式。

#### 2.10 `rflux-verify` – 验证与一致性检查

- **逻辑等价**：检查综合前后功能一致性，并验证 splitter/DFF 插入不破坏周期级行为。
- **结构一致性**：检查单消费约束、时钟域边界、宏单元接口合法性和偏置网络连通性。
- **联合验证接口**：为电-热-力协同优化（偏置稳定性、Integration/Interfaces 类问题）与制造规则检查预留统一报告格式；DC 偏置网络寄生电感对偏置电压的影响纳入远期 STA/验证边界。

#### 2.11 `rflux-io` – 文件格式支持

| 格式 | 读 | 写 | 用途 |
|------|----|----|------|
| Verilog (subset) | ✅ | ❌ | RTL 输入 |
| BLIF | ✅ | ✅ | 逻辑综合中间 |
| LEF/DEF | ✅ | ✅ | 布局布线交换 |
| GDSII | ❌ | ✅ (规划中) | 最终版图 (可对接 `libreda_db`) |
| SPICE | ✅ | ✅ | 器件模型和网表 |
| JSON | ✅ | ✅ | 配置和报告 |

#### 2.12 Python 绑定（`rflux-py` + `python/rflux`）

**定位**：Rust 实现核心能力，Python 提供易用 API、Notebook 与外部工具胶水；**不在 Python 中重复实现**综合、STA、布线等重逻辑。

| 组件 | 路径 | 职责 |
|------|------|------|
| PyO3 扩展 | `crates/py`（`rflux-py`） | 暴露 `Circuit`/`Netlist` 读写、时序报告解析、调用 `rflux-cli` 子命令、零拷贝 NumPy 缓冲区（远期） |
| 纯 Python 包 | `python/rflux` | 类型友好封装、`pathlib` 工作流、ColdFlux/qPALACE 目录约定适配 |
| 构建 | maturin + uv | `uv run maturin develop -m crates/py/Cargo.toml`；CI 用 `maturin build` 产出 wheel |
| 依赖管理 | uv | 根 `pyproject.toml` + `uv.lock`；禁止裸 `pip install`（见 AGENTS.md） |

**分阶段暴露 API**：

| 阶段 | Python API（示例） | 依赖 Rust crate |
|------|-------------------|-----------------|
| P0 | 读写信令 JSON IR、加载 PDK 元数据 | `rflux-ir`, `rflux-tech` |
| P1 | `compile()` 驱动综合、返回网表对象 | `rflux-synth`, `rflux-io` |
| P2 | `sta()` / 时序报告 `TimingReport` | `rflux-timing` |
| P3 | `place_route()` 或分步 `place()` / `route()` | `rflux-place`, `rflux-route` |
| P4 | JoSIM 批跑封装、`compare_bench(qpalace)` | `rflux-sim`, scripts |

**集成场景**：

- **ColdFlux / qPALACE**：Python 脚本生成/消费 LEF/DEF、JSON 报告，与现有 Python 驱动流程并列调用 `rflux` CLI 或库。
- **科研与基准**：`python/notebooks` 对比后布线频率/面积；`python/scripts` 批量回归。
- **REPL / 测试**：`uv run pytest`；可选 `ipython`（dev 依赖）。

**错误与稳定性**：PyO3 边界将 Rust `Result` 映射为 Python 异常（`RfluxError` 层次）；公开 API 遵循 semver，与 `rflux-*` crate 版本对齐（同 workspace 版本号）。

### 三、五层核心设计流程（与 crate 映射）

自底向上，`rflux` 工具链可理解为五个相互依赖的层次（与 `sfq.md` 技术路线一致）：

| 层次 | 内容 | 主要 crate | 关键产出 |
|------|------|------------|----------|
| **L1** | 器件库 + 脉冲级 IR | `rflux-device`, `rflux-ir`, `rflux-hdl`, `rflux-tech` | 类型安全的网表、PDK 规则、可选 SPICE 子网表 |
| **L2** | 时序驱动布局布线 | `rflux-place`, `rflux-route` | levelized placement、JTL/PTL 混合路由、CTS 原型 |
| **L3** | 分层协同仿真 | `rflux-sim` | JoSIM + 事件驱动混合、跨模块 PTL 检查 |
| **L4** | 保持时间违例修复 | `rflux-timing` + `rflux-route` | STA + JTL 插入优化器 |
| **L5** | AC 供电分析与优化 | `rflux-device`, `rflux-route`, `rflux-timing` | 偏置方案自动权衡（高级特性） |

**技术攻坚优先级**（结合 Rust 生态成熟度与复用价值）：

1. 器件建模 + IR（成本最低、类型系统收益最大）
2. 逻辑综合（内建纯 Rust 布尔优化，扩展 Splitter / 平衡 DFF）
3. 时序驱动 P&R（复用 `libreda_pnr` trait，自研 SFQ 算法——**相对 CMOS 的最大差异点**）
4. SPICE 级仿真扩展（扩展 `thevenin`：JJ + 超导传输线）
5. 时钟树综合 + AC 偏置优化（与供电紧耦合，放在后期）

### 四、实现路线图

#### Phase 0 – 基础设施 (1-2 月)
- 建立 workspace，定义各 crate 接口。
- 初始化 **uv**：根 `pyproject.toml`、`uv.lock`、`.python-version`；约定见 `AGENTS.md`。
- 搭建 `crates/py`（maturin + PyO3 骨架）与 `python/rflux` 包，暴露最小 API（如版本号、空 `Circuit`）。
- 实现 `rflux-ir` 核心数据结构（含单消费/所有权约束原型）。
- 定义 `rflux-tech` 的工艺/PDK 抽象与基础设计规则查询接口（含 PTL 危险长度区间占位）。
- 实现 `rflux-io` 读写 LEF/DEF（调用 `libreda_db`）。
- 调研 RustSFQ / cmtrs API，确定 `rflux-hdl` 最小 DSL 边界。

#### Phase 1 – 逻辑综合 (2-3 月)
- 实现内建纯 Rust 布尔优化与逻辑共享合并。
- 实现技术映射 + 分裂器插入。
- 实现路径平衡 DFF 插入。
- 支持复合单元/宏单元映射与层次化网表输出。
- 输出 `rflux-ir` 网表。

#### Phase 2 – 布局布线 (3-5 月)
- 实现 levelized placement，并将输入/输出端口推到左右边界，同时支持显式 blocked-region 避障。
- 为 `MacroCell` 提供 halo 约束，并加入最小 congestion-aware level spill 合法化。
- 实现 JTL 布线器 (当前为最小 Manhattan bootstrap，并保持边界端口 access net 为 JTL，同时支持显式 keep-out 绕障)。
- 将 JTL 路由升级为基于障碍网格的最短路搜索。
- 实现 PTL 长度受限布线。
- 打通 `rflux-flow` 端到端 `synth -> place -> route` 最小闭环，并加入一次基于 detour 开销的反馈迭代。
- 实现门级时钟分发和双相位/多相位时钟树原型。
- 集成基本时序驱动迭代优化与最小 hold-fix reroute 原型。

#### Phase 3 – 时序与验证 (2-3 月)
- 当前原型范围：已完成。
- 实现 STA 引擎（查表 + 线性插值）。当前已落地 `rflux-timing` 原型，并接入 `rflux-flow` 输出 setup/hold slack、critical path 与 violation 统计，同时提供独立 `analyze_timing` API。
- 已支持节点级到达/要求时间覆盖与 clock-domain 周期约束原型。
- 已支持 `PinRef` 级时序约束，允许端口约束覆盖节点级约束。
- 已支持显式跨时钟域约束原型，包括 `false_path`、`max_delay` 与 `multicycle`。
- 已在时序摘要中保留 `false_path` 跨域弧计数，避免异常路径仅体现在 setup violation 统计缺失上。
- 已通过 `analyze_timing` 暴露精简的逐弧时序记录，便于 Python 侧直接查看 crossing 例外、route length、时钟域上下文与 setup/hold slack。
- 实现保持时间违例自动修复。
- 实现混合仿真器（调用 JoSIM + 事件驱动）。当前已提供事件驱动摘要与外部仿真器 hook。
- 当前外部仿真路径已支持生成临时瞬态 deck，并回收基础结果字段、波形路径、违例计数、最差延迟标量以及带端点上下文的枚举化 delay / violation 明细。
- 实现跨宏单元 PTL 电气检查与结构一致性验证。当前已提供 `verify_layout` 的宏边界 PTL / forbidden-length / structural check 原型。
- 基准测试对比 qPALACE / ColdFlux 结果。

#### Phase 4 – 高级特性 (3 月+)
- 当前状态：已完成。
- AC 偏置分析与优化（当前已具备 timing-aware 双参数轻量选解原型）。
- 统计时序分析（当前已具备跨域分类、device-aware 与 route-aware sigma 原型）。
- 复合单元自动特征化（当前已具备通过仿真 hook 生成 timing-library-ready 摘要的原型）。
- Rust-native 编译期 SFQ 硬件描述 DSL（当前已具备最小 builder DSL 原型）。
- 电-热-力联合分析接口与制造约束检查（当前已具备高级约束分析原型）。

#### Phase 5 – 库工件与反馈集成 (已完成)
- 当前状态：已完成（见 [phase-5.md](./phase-5.md)）。
- 将复合单元自动特征化从 timing-library-ready 摘要推进为可复用技术库工件；当前已能生成 cell/timing library artifact，并通过 `rflux-tech` 的 JSON 回灌入口接入 PDK 更新路径。
- 当前技术库已支持按 cell name 的 timing override，避免 characterization 结果粗粒度覆盖整个 `SfCellKind` 默认模型。
- 当前 `rflux-synth` / `rflux-timing` / `rflux-flow` 已能优先消费这些 name-indexed 工件，从而把 characterization 反馈进时序、统计时序、AC bias 和高级约束分析。
- 当前已支持 `CharacterizedCellLibraryBundle` 多工件合并、波形校准元数据，以及由 characterized cell 面积/流水深度驱动的 placement halo 与 routing 成本缩放。
- 弧级 `arc_delays` 已接入 STA；`optimize_design_with_characterized_library` 联合 SSTA/约束评分；见 [phase-5-workflow.md](./phase-5-workflow.md)。
- 已完成：仿真弧延迟名称启发式匹配；`optimize_design_with_characterized_library` 协同优化 routing、placement halo 与 SSTA sigma。

### 五、依赖与复用策略

Rust EDA 生态尚不成熟，但以下组件可筑基；**差距项须自研**。

| 功能 | 外部 crate | 使用方式 | 评价与差距 |
|------|------------|----------|------------|
| 布尔逻辑综合 | 仓库内置纯 Rust 实现 | 直接集成 | 当前提供共享子表达式合并、扁平化与兼容性分析；后续再补更强等价性和模式识别 |
| P&R 框架 | `libreda_pnr` | 实现 trait | **最有价值复用点**：`PlacementProblem`、`RoutingProblem`、`TimingAnalysis` 等；**不含** JTL/PTL 混合路由与脉冲 STA |
| 版图/交换格式 | `libreda_db` | 直接复用 | OASIS、LEF/DEF 等，服务 SFQ PDK 与 DEF 交换 |
| SPICE 仿真 | `thevenin` | 扩展设备模型 | MNA 求解器可扩展 RCSJ/RCSJ；当前偏 CMOS；native/WASM |
| Rust 内嵌 HDL | `cmtrs`, `fayalite` | 参考 DSL 设计 | `rflux-hdl` proc_macro 前端 |
| ODE/PDE / 场仿真 | `numra` | 可选集成 | flux trapping、磁场分析（远期） |
| 数值计算 | `nalgebra`, `rayon` | 布局 QP、并行 | |
| 仿真胶水 | `std::process` / FFI | JoSIM | 精确子电路；需用户安装 JoSIM |
| 序列化 / CLI / 日志 | `serde`, `clap`, `tracing` | 标配 | |
| Python 绑定 | `pyo3`, `maturin` | `crates/py` | 与 workspace 同版本发布 wheel |
| Python 工具链 | **uv** | 根目录锁定依赖 | 见 `AGENTS.md`；dev: `pytest`, `ruff`（可选） |

**外部引擎备选**：布线可评估对接 TritonRoute 等（通过 `libreda_pnr` 抽象），但 SFQ 的 JTL/PTL 规则须由 `rflux-route` 主导约束生成。

### 六、阶段性聚焦建议

- **MVP 边界**：优先实现 `rflux-ir` + `rflux-synth` + `rflux-timing` 的最小闭环，完成 splitter 插入、路径平衡和脉冲窗口 STA。
- **差异化核心**：布局布线阶段优先落地 JTL/PTL 混合路由和保持时间修复，这两部分最能体现 SFQ 相对通用 P&R 框架的专用价值。
- **高级功能后置**：AC 偏置优化、材料级联合分析和全流程 DSL 在第一阶段不追求一次到位，先为接口留钩子。

### 七、实施策略（个人与小团队）

完整 SFQ EDA 链涉及超导物理、数字设计、EDA 算法、计算几何、数值仿真等多学科，**单点突破**比一次性全栈更可行：

| 策略 | 说明 |
|------|------|
| **专注单环** | 例如：仅做 SFQ 保持时间 JTL 插入优化器、或专用 CTS，输出 LEF/DEF/JSON 与 ColdFlux 流程对接 |
| **复用 ColdFlux/qPALACE** | 综合与全流程已有五年+积累；注意 PDK 绑定与跨工艺限制 |
| **Rust 替换热点** | 仿真引擎、时序优化、PTL 路由长度计算等计算密集模块，经 FFI 逐步替换 Python/C++ 瓶颈 |
| **rflux 默认路径** | 以 crate 库 + CLI 形式交付可嵌入组件，而非要求用户放弃现有 qPALACE 流程 |

### 八、项目命名与风格

- **主仓库名**：`rflux`
- **命令行工具**：`rflux` 家族 (如 `rflux compile`, `rflux place`, `rflux route`, `rflux sta`)
- **Logo 概念**：蓝色流线 + 闪电 (表示磁通量子) + Rust 齿轮

### 九、开源与协作策略

- 许可证：Apache 2.0 或 MIT + 专利授权（避免超导 EDA 关键算法被闭源锁死）。
- 文档托管：https://rflux.dev （初期使用 GitHub Pages + mdBook）
- 社区：与 ColdFlux 团队、IEEE CSC 超导计算社区建立联系，争取联合开发。

---

## 总结

SFQ EDA 与传统 CMOS EDA 的差异不在于“多几个功能”，而在于逻辑表示、时序约束、物理设计与验证方法均需**自底向上重新设计**；无法仅靠扩展现有 CMOS 框架弥补。

`rflux` 旨在填补 Rust 生态中无系统级 SFQ EDA 工具的空白，并以模块化 crate 逐步构建：

- **差异化核心**：脉冲级 IR（含所有权/单消费约束）、数据流-时钟紧耦合综合、JTL/PTL 混合路由、脉冲窗口 STA 与 JTL 保持时间修复、分层混合仿真。
- **生态复用**：`libreda_pnr`（P&R 抽象）、`thevenin`（SPICE 扩展）、`libreda_db`（版图交换）；布尔综合保持仓库内纯 Rust 实现。
- **务实路径**：与 ColdFlux/qPALACE 共存、优先交付可嵌入的高性能子模块，而非一期替代全流程。
- **Python 生态**：uv + PyO3 降低脚本与现有 Python EDA 流程的接入成本，核心仍保持在 Rust。

长远目标是成为超导数字电路设计的 **开放、可扩展、高性能** 基础工具链；Rust 负责核心计算，Python 负责集成与交互，二者通过 maturin 绑定与 uv 统一环境协同。

---

## 相关文档

- [sfq.md](./sfq.md) — SFQ EDA 与 CMOS 差异分析、Rust 技术路线与优先级（本文档的设计依据）
- [phase-3.md](./phase-3.md) — Phase 3 当前进展与剩余工作
- [AGENTS.md](../AGENTS.md) — AI/贡献者协作说明（**uv** 与 Python 使用规则）