# rflux 功能补全与改善计划

> 基于 `SFQ-EDA-Deep-Dive.md` 的六层差异分析，对照当前 18 个 crate 的实现现状，识别差距并制定改善计划。

---

## 现状评估

rflux 当前 18 个 crate 均已完成功能实现（无 stub/TODO），覆盖完整 EDA 流程：Verilog/BLIF/EDIF/SPICE 输入 → 综合 → 布局 → CTS → 布线 → STA/SSTA → DRC/LVS → 仿真 → GDS/SVG 输出。

以下改善计划聚焦于**从"能跑通"到"真正解决 SFQ 独特问题"**的深化，而非从零搭建。

---

## 优先级定义

| 级别 | 含义 |
|------|------|
| **P0** | SFQ 相对 CMOS 的本质差异点，不实现则工具链无法解决真正的 SFQ 问题 |
| **P1** | 显著提升工具链在 SFQ 场景下的精度或自动化程度 |
| **P2** | 改善工程质量和生态完整性 |

---

## P0：核心差异化能力深化

### P0-1：IR 语义升级——从网表到脉冲数据流图

**现状**: `rflux-ir` 的 `Netlist` 是带 `NodeKind` 标注的有向图，本质仍是门级网表。时钟信息通过 `PinRef` 外挂，脉冲窗口、路径平衡约束不在 IR 层表达。

**目标**: IR 层直接建模 SFQ 的脉冲数据流语义，使综合、布局、时序分析共享统一的语义基础。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| 新增 `PulseEdge` 结构体 | `rflux-ir` | 每条边携带脉冲有效窗口 `[t_min, t_max]`、传播延迟模型、路径平衡标记 |
| 新增 `ClockDomain` 枚举 | `rflux-ir` | 显式编码时钟域、相位、频率，挂载到节点而非仅在 timing 层 |
| 新增 `PathBalanceConstraint` | `rflux-ir` | 从 synth 的分析结果提升为 IR 一等约束，供 downstream 消费 |
| `NodeKind` 扩展 | `rflux-ir` | `MacroCell` 增加时钟边界端口标记（哪些端口接时钟、哪些是数据） |
| IR JSON schema 升级 | `rflux-io` | 新增 `pulse_edges`、`clock_domains`、`balance_constraints` 段，保持向后兼容 |
| `CircuitBuilder` 扩展 | `rflux-hdl` | 支持在构建时指定脉冲窗口和时钟域 |

**验证标准**: IR JSON 序列化/反序列化后，脉冲窗口和时钟域信息无损；synth 输出的 IR 包含完整的 balance constraint。

---

### P0-2：时钟路由一体化——时钟与信号共享布线资源竞争

**现状**: `rflux-flow` 的 CTS 是独立阶段（`build_h_tree`），在布局后、布线前执行。时钟树生成后作为固定约束传给路由器。时钟路径不参与布线资源竞争。

**目标**: 时钟作为物理脉冲信号，与数据信号在布线资源上竞争。CTS 不应是独立阶段，而应与路由深度耦合。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| 时钟路由纳入 `SimpleRouter` | `rflux-route` | 新增 `ClockNet` 标记，时钟网络与数据网络在同一轮路由中竞争通道资源 |
| 时钟路径延迟参与 STA | `rflux-timing` | 时钟到达时间不再是 CTS 给定的理想值，而是路由后的实际 JTL 延迟 |
| CTS 改为路由前置评估 | `rflux-flow` | `build_h_tree` 输出时钟路由候选方案，路由器评估资源冲突后选择最优方案 |
| 多相位时钟路由 | `rflux-route` | 支持双相位/多相位时钟的不同物理路径，每个门的时钟相位由 IR 层指定 |
| 时钟 skew 从"最小化"改为"精确控制" | `rflux-flow` | 不同时钟域允许不同 skew 目标，关键路径时钟可有意提前/延后 |

**验证标准**: 同一布线网格中，时钟网络和数据网络的通道使用率之和不超过 100%；STA 中的时钟到达时间来自实际路由结果而非理想假设。

---

### P0-3：功能-时序联合验证

**现状**: `rflux-verify` 仅做 SAT-based 等价性检查（组合/时序），不涉及时序约束。`rflux-timing` 的 STA 输出 slack 和 violation，但不与功能正确性关联。

**目标**: 验证"在所有工艺角下，脉冲窗口的对齐关系保证电路行为正确"——时序问题直接就是功能问题。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| 新增 `TimingFunctionalVerifier` | `rflux-verify` | 接收 IR + timing report，检查所有路径的脉冲窗口重叠是否满足功能正确性 |
| 路径平衡功能验证 | `rflux-verify` | 验证所有 combinational 路径的延迟差不超过一个时钟周期（SFQ destructive readout 约束） |
| 多角功能一致性 | `rflux-verify` | 在每个 timing corner 下运行功能验证，确保工艺偏差不破坏脉冲对齐 |
| 验证报告与 timing report 交叉引用 | `rflux-verify` + `rflux-timing` | violation 路径自动关联到 timing arc，便于定位根因 |
| CLI 子命令 `verify-timing-functional` | `rflux-cli` | 新增入口，输出联合验证报告 |

**验证标准**: 对一个有意插入 hold violation 的测试电路，联合验证能同时报告功能错误（路径不平衡）和时序违规（hold slack < 0）。

---

### P0-4：JTL/PTL 路由决策深化——从规则到电气模型

**现状**: `rflux-route` 的 `SimpleRouter` 基于长度阈值选择 JTL/PTL，有反射分析（`ReflectionAnalyzer`）和 forbidden range 检查。但决策逻辑是规则驱动的（长度 > 阈值 → PTL，否则 JTL），缺乏基于电气模型的全局优化。

**目标**: 路由选择基于电气特性（延迟、反射、功耗、面积）的全局权衡，而非简单的长度阈值。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| 路由代价函数重构 | `rflux-route` | 代价函数从 Manhattan 距离 + congestion 扩展为 `α·delay + β·reflection_risk + γ·area + δ·coupling`，权重可配置 |
| PTL 电气长度预估 | `rflux-route` + `rflux-extract` | 布线阶段即调用寄生提取预估 PTL 特征阻抗和反射系数，而非布线后检查 |
| JTL/PTL 混合路径全局优化 | `rflux-route` | 新增 `HybridRouteOptimizer`：对整条信号路径（而非逐段）做 JTL/PTL 组合选择，最小化端到端延迟和反射风险 |
| PTL 危险长度区间建模细化 | `rflux-tech` | PDK 中的 `ptl_forbidden_lengths` 从离散区间扩展为连续的反射风险函数，供路由代价函数使用 |
| 路由后电气验证增强 | `rflux-route` | 路由完成后自动运行反射和串扰检查，不满足则触发局部 reroute |

**验证标准**: 对同一网表，混合路由优化器的后布线频率优于简单长度阈值路由 >5%。

---

## P1：精度与自动化提升

### P1-1：分层混合仿真调度器

**现状**: `rflux-sim` 有 `SimulationDepthAnalysis` 可分析层次深度，支持 `EventOnly`、`ExternalJosim`、`InternalTransient` 三种后端。但选择策略是手动配置（`SimulationMode`），未实现自动的层次化调度。

**目标**: 自动决定哪些子电路需要 SPICE 级精度、哪些用事件驱动抽象，并管理两种仿真域的接口。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| `HierarchicalScheduler` | `rflux-sim` | 基于 `SimulationDepthAnalysis` 的输出，自动划分仿真域：≤3 级 → JoSIM，>3 级 → 事件驱动，跨宏 PTL → JoSIM |
| 子电路自动提取 | `rflux-sim` | 从 IR 中提取 SPICE 域子电路，自动生成 JoSIM deck，无需用户手动准备 |
| 仿真域接口一致性 | `rflux-sim` | 事件驱动仿真器的脉冲输出自动转换为 JoSIM 的电压波形输入（反之亦然），保证域边界信号连续 |
| 增量仿真 | `rflux-sim` | 布局变更后仅重新仿真受影响的子电路，复用未变更部分的结果 |
| CLI `simulate-hierarchical` | `rflux-cli` | 新增层次化仿真入口，自动调度并输出统一报告 |

**验证标准**: 对 c432 基准电路，层次化仿真时间 < 全 JoSIM 仿真的 1/10，且关键路径延迟误差 < 10%。

---

### P1-2：AC/DC 偏置自动设计空间探索

**现状**: `rflux-flow` 有 `analyze_ac_bias()` 和 `optimize_ac_bias()`，可以做 AC/DC 参数对比。但探索是双参数轻量级的，缺乏系统化的 Pareto 前沿搜索。

**目标**: 自动搜索 AC/DC 偏置方案的 Pareto 前沿（功耗 vs 面积 vs 频率），输出可选方案集合供设计者决策。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| `BiasDesignSpaceExplorer` | `rflux-flow` | 多目标优化器，以 JJ 数量、面积、最大频率、静态功耗为目标，搜索 AC 占比 × 偏置网络拓扑的设计空间 |
| 偏置拓扑生成器 | `rflux-route` | 自动生成多种偏置网络拓扑方案（纯 DC、纯 AC、混合 AC/DC 分区），供探索器评估 |
| 成本模型细化 | `rflux-tech` | PDK 中增加偏置相关的成本参数：AC 转换器面积开销、偏置线寄生电感模型 |
| Pareto 前沿可视化 | `rflux-io` | 输出功耗-面积-频率的 Pareto 前沿 JSON + SVG 图 |
| CLI `explore-bias` | `rflux-cli` | 新增偏置探索子命令 |

**验证标准**: 对给定 PDK，探索器能在 100 个方案内收敛到 Pareto 前沿，且前沿方案的功耗/面积/频率权衡覆盖从纯 DC 到纯 AC 的完整范围。

---

### P1-3：SSTA 从简化到完整

**现状**: `rflux-timing` 的 SSTA 使用 local/global sigma 传播，是解析矩方法的简化版本。`rflux-margin` 的 Monte Carlo 可以做 yield 估计，但与 SSTA 是独立的。

**目标**: SSTA 支持完整的路径-based 统计时序分析，与 Monte Carlo 结果交叉验证。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| 路径-based SSTA | `rflux-timing` | 实现 A/B/C 矩阵方法（或 SSTA 经典的 canonical form），对关键路径做相关性感知的统计叠加 |
| 工艺偏差模型扩展 | `rflux-tech` | 支持 J_c、线宽、膜厚等多参数联合分布，而非单参数 sigma |
| SSTA 与 Monte Carlo 交叉验证 | `rflux-margin` | 新增 `validate_ssta()`：对比 SSTA 预测的 yield 与 Monte Carlo 实际结果，报告偏差 |
| 最差情况角自动生成 | `rflux-timing` | 从 SSTA 结果中提取最差工艺角，生成确定性 STA 可用的 corner 参数 |
| 统计时序报告增强 | `rflux-timing` | 输出每条路径的均值/sigma/偏度，而非仅 slack 均值 |

**验证标准**: SSTA 预测的 3σ slack 与 10000 样本 Monte Carlo 的 3σ slack 偏差 < 10%。

---

### P1-4：PTL 传输线效应在时序分析中的建模

**现状**: `rflux-timing` 的 interconnect 延迟模型是集总参数（RC 或固定延迟/um）。PTL 的传输线效应（反射、振铃、阻抗不连续）在 `rflux-route` 的 `ReflectionAnalyzer` 中有检查，但未反馈到 STA 的延迟计算中。

**目标**: STA 中的 PTL 延迟使用分布参数传输线模型，而非集总 RC 近似。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| 传输线延迟模型 | `rflux-timing` | 新增 `TLineDelayModel`：基于特征阻抗、传播常数和端接条件计算脉冲传播延迟和反射分量 |
| 延迟-长度非线性表 | `rflux-tech` | PDK 中提供 PTL 延迟 vs 长度的非线性查找表（含反射修正），替代线性近似 |
| 阻抗不连续点标注 | `rflux-route` | 路由器在 JTL/PTL 交界处标注阻抗不连续，STA 据此插入反射延迟修正 |
| 时序报告中的反射裕量 | `rflux-timing` | 每条 PTL 路径的 timing report 新增 `reflection_margin` 字段 |

**验证标准**: 对一条长 PTL 路径，传输线模型的延迟预测与 JoSIM 瞬态仿真结果偏差 < 15%。

---

### P1-5：综合阶段的布线可行性早期评估

**现状**: `rflux-synth` 的流程是布尔优化 → 技术映射 → splitter/DFF 插入，不考虑物理约束。物理可行性要等到布局布线阶段才能发现。

**目标**: 综合阶段就做粗略的布线可行性评估，避免生成物理上不可行的网表。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| `PhysicalFeasibilityEstimator` | `rflux-synth` | 基于单元面积和扇出数量，估算布线通道需求和拥塞风险 |
| 扇出分裂器布局感知插入 | `rflux-synth` | splitter 插入时考虑物理位置（输入端口方向），减少后续布线绕行 |
| 复合单元映射的面积-时序权衡 | `rflux-synth` | tech mapping 的目标函数从纯面积扩展为 `α·area + β·estimated_wire_length + γ·pipeline_depth` |
| 与 flow 的反馈接口 | `rflux-flow` | 新增 `synth_physical_feedback`：综合完成后快速评估物理可行性，不满足则触发综合参数调整重试 |
| 粗略布局估算 | `rflux-place` | 提供 `estimate_layout()` 快速接口（不跑完整 SA，仅用 quadratic placement 估算线长） |

**验证标准**: 对一个拥塞严重的测试电路，综合阶段的可行性评估能在 1 秒内识别拥塞风险，并建议减少扇出或增加复合单元映射。

---

## P2：工程质量与生态

### P2-1：Rust 类型系统编码物理约束

**现状**: JTL/PTL 布线段使用同一 `RouteSegment` 结构体，通过 `RouteMode` 枚举区分。时钟相位通过数值字段表示，无编译期保证。

**目标**: 利用 Rust 类型系统在编译期防止物理约束违规。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| `JtlSegment` / `PtlSegment` 类型 | `rflux-route` | 用独立类型替代 `RouteMode` 枚举，编译期防止 JTL/PTL 混用 |
| `ClockPhase<const N: usize>` 泛型 | `rflux-ir` | 用 const 泛型编码时钟相位编号，编译器检查相位匹配 |
| `ProcessCorner` 标记类型 | `rflux-tech` | 用 `PhantomData` 标记不同工艺角的 PDK 参数，防止跨角混用 |
| 单消费约束的生命周期保证 | `rflux-ir` | 用引用计数 + 编译期检查强化"每个输出只被一个输入消费"的不变量 |

**验证标准**: 故意混用 JTL/PTL 类型的代码无法通过编译。

---

### P2-2：SFQ 可测试性（DFT）框架

**现状**: 无任何 DFT 相关功能。SFQ 的 destructive readout 使传统扫描链不直接适用。

**目标**: 提供 SFQ 特有的可测试性基础设施。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| `TestPointInjector` | `rflux-synth` | 在关键路径节点插入可控的脉冲注入点，用于功能测试 |
| `PulseObserver` | `rflux-synth` | 在输出端口或观察点插入非破坏性脉冲检测器（复制脉冲到观察通道） |
| 测试模式生成 | `rflux-verify` | 基于 SAT 求解生成测试向量，确保每个门的输入脉冲可被激活 |
| 可测试性分析报告 | `rflux-verify` | 报告测试覆盖率、不可控/不可观测节点列表 |
| `TestableDesign` 包装器 | `rflux-hdl` | DSL 层面支持 `#[testable]` 标注，自动插入测试基础设施 |

**验证标准**: 对 ISCAS c432 基准电路，插入测试点后的 stuck-at 故障覆盖率 > 90%。

---

### P2-3：仿真精度提升——JJ 器件模型扩展

**现状**: `rflux-sim` 的内部 transient 仿真使用线性 RC 模型，外部 JoSIM 集成依赖用户安装。`rflux-tech` 的器件模型覆盖 JJ 基本参数，但 SPICE 模型卡的生成较为简单。

**目标**: 提升内部仿真的器件模型精度，减少对外部 JoSIM 的依赖。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| RCSJ 模型实现 | `rflux-sim` | 在内部 transient 引擎中实现 Josephson 结的 RCSJ（Resistively and Capacitively Shunted Junction）模型，替代线性 RC |
| 超导传输线模型 | `rflux-sim` | 实现分布参数超导传输线的等效电路模型（基于伦敦方程和 kinetic inductance） |
| 0-JJ / π-JJ 区分 | `rflux-sim` + `rflux-tech` | 仿真器支持两种 JJ 类型的不同 I-V 特性和相位偏移 |
| 模型卡自动特征化 | `rflux-tech` | 从 PDK 参数自动生成 SPICE 模型卡，包含 JJ 的 Ic、Rn、Cj 和温度依赖 |
| 内部仿真与 JoSIM 交叉验证 | `rflux-sim` | 新增 `validate_against_josim()`：对同一电路运行内部仿真和 JoSIM，对比波形偏差 |

**验证标准**: 对单个 JJ 的基本开关特性，内部 RCSJ 模型与 JoSIM 的波形偏差 < 5%。

---

### P2-4：端到端流程的跨阶段反馈迭代

**现状**: `rflux-flow` 的 `compile_layout` 有 timing closure loop（synth → place → CTS → route → timing → hold fix → reroute → re-timing），但反馈是单向的（timing → route），且只修复 hold violation。

**目标**: 支持多轮跨阶段反馈，包括 timing → synth（重新映射）、timing → place（重新布局）、congestion → synth（减少扇出）。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| `FeedbackAction` 枚举 | `rflux-flow` | 定义可反馈动作：`RemapCell`、`RebalancePaths`、`RePlaceRegion`、`ReduceFanout`、`InsertBuffer` |
| timing → synth 反馈 | `rflux-flow` + `rflux-synth` | 当 setup slack 不满足时，尝试重新映射关键路径上的单元（换更快的 cell 或减少流水深度） |
| congestion → synth 反馈 | `rflux-flow` + `rflux-synth` | 当局部拥塞超标时，触发扇出分裂器重新插入或复合单元替换以减少布线需求 |
| place → synth 反馈 | `rflux-flow` | 布局后发现关键路径跨度过长时，反馈给综合器要求减少该路径的逻辑深度 |
| 迭代收敛策略 | `rflux-flow` | 最大迭代次数、改善阈值、动作优先级（先布局后综合）的可配置收敛策略 |
| 迭代历史报告 | `rflux-flow` | 输出每轮迭代的动作、slack 变化、面积变化，便于调试 |

**验证标准**: 对一个 setup violation 的测试电路，跨阶段反馈能在 3 轮迭代内消除 violation，且面积开销 < 10%。

---

### P2-5：基准测试与对标

**现状**: 有 ISCAS c17/c432、pipeline、NAND/MAJ chain 基准测试，以及 criterion 性能基准。但缺少与 ColdFlux/qPALACE 的对比。

**目标**: 建立可重复的对标流程，量化 rflux 与 ColdFlux 在关键指标上的差异。

**具体改动**:

| 改动 | 涉及 crate | 说明 |
|------|------------|------|
| ColdFlux 输出解析器 | `rflux-io` | 解析 ColdFlux/qPALACE 的输出格式（时序报告、布局文件），转换为 rflux 可比较的格式 |
| 对标脚本 | `python/scripts/` | `benchmark_vs_coldflux.py`：同一基准电路跑两个工具，对比后布线频率、面积、仿真时间 |
| ISCAS 完整套件 | `rflux-cli` | 扩展基准到 ISCAS 全套（c432/c499/c880/c1355/c1908/c2670/c3540/c5315/c6288/c7552） |
| 结果数据库 | `python/scripts/` | JSON 格式的结果历史记录，支持趋势分析 |
| CI 集成 | `.github/` | PR 自动运行基准回归，检测性能退化 |

**验证标准**: 每个 ISCAS 基准电路都有 rflux 和 ColdFlux 的后布线频率/面积对比数据。

---

## 改善计划总览

| 编号 | 名称 | 优先级 | 涉及 crate | 预估工作量 |
|------|------|--------|-----------|-----------|
| P0-1 | IR 语义升级：脉冲数据流图 | P0 | ir, io, hdl | 2-3 周 |
| P0-2 | 时钟路由一体化 | P0 | route, timing, flow | 3-4 周 |
| P0-3 | 功能-时序联合验证 | P0 | verify, timing, cli | 2-3 周 |
| P0-4 | JTL/PTL 路由决策深化 | P0 | route, tech, extract | 2-3 周 |
| P1-1 | 分层混合仿真调度器 | P1 | sim, cli | 2-3 周 |
| P1-2 | AC/DC 偏置自动探索 | P1 | flow, route, tech, io, cli | 2-3 周 |
| P1-3 | SSTA 完整化 | P1 | timing, tech, margin | 2-3 周 |
| P1-4 | PTL 传输线时序建模 | P1 | timing, tech, route | 1-2 周 |
| P1-5 | 综合阶段布线可行性评估 | P1 | synth, flow, place | 1-2 周 |
| P2-1 | 类型系统编码物理约束 | P2 | route, ir, tech | 1 周 |
| P2-2 | SFQ DFT 框架 | P2 | synth, verify, hdl | 3-4 周 |
| P2-3 | JJ 器件模型扩展 | P2 | sim, tech | 2-3 周 |
| P2-4 | 跨阶段反馈迭代 | P2 | flow, synth, place, route | 3-4 周 |
| P2-5 | 基准测试与对标 | P2 | io, cli, python/scripts | 2 周 |

---

## 建议执行顺序

**第一轮（4-6 周）**: P0-1 → P0-3 → P0-4
- IR 语义升级是所有后续改善的基础
- 功能-时序联合验证是 SFQ 最独特的验证需求
- JTL/PTL 路由决策深化直接影响后布线质量

**第二轮（4-6 周）**: P0-2 → P1-4 → P1-5
- 时钟路由一体化依赖 P0-1 的 IR 时钟域信息
- PTL 传输线建模提升时序精度
- 综合可行性评估减少迭代次数

**第三轮（4-6 周）**: P1-1 → P1-2 → P1-3
- 混合仿真调度器提升验证效率
- AC/DC 探索和 SSTA 完整化提升设计空间覆盖

**第四轮（按需）**: P2-1 → P2-2 → P2-3 → P2-4 → P2-5
- 类型安全和 DFT 是长期质量投资
- 器件模型扩展和跨阶段反馈是高级特性
- 基准对标是生态建设
