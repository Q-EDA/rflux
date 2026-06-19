# rflux Roadmap

rflux 是面向 SFQ（单磁通量子）超导数字电路的 EDA 后端工具链。

## 当前状态

18 个 Rust crate，覆盖完整设计流程：IR → 综合 → P&R → STA/SSTA → 仿真 → 验证。

---

## 已完成

### 第一阶段：核心能力补齐

- ✅ Verilog 前端（Phase 1-3：结构化 + 行为级 + generate/task/function）
- ✅ DRC/LVS 物理验证
- ✅ 寄生提取（R/C/L from layout geometry）
- ✅ Waveform-aware 时序分析
- ✅ OCV derating
- ✅ 噪声裕度分析
- ✅ 串扰系数可配置化
- ✅ BLIF/EDIF/SPICE 输入格式
- ✅ 技术映射增强
- ✅ 错误码统一

### 第二阶段：算法升级

- ✅ 模拟退火布局器（HPWL + 拥塞 + 时序代价函数）
- ✅ 时钟树综合增强（buffer sizing、skew 优化、clock gating）
- ✅ 串扰感知布线（coupling_weight in Dijkstra）
- ✅ 功耗分析（per-cell JJ count × Ic × V × f）
- ✅ 良率优化（设计中心化）

### 第三阶段：时序与验证增强

- ✅ 关键路径枚举（top-K paths）
- ✅ 时序驱动布局优化
- ✅ 多角时序合并
- ✅ SSTA 蒙特卡洛验证
- ✅ 几何 DRC 扩展（金属密度、天线效应）
- ✅ LVS 增强（参数匹配、层次化对比）
- ✅ DRC 规则引擎（PDK YAML 加载）
- ✅ DRC 违例 SVG 可视化
- ✅ 时序驱动 DRC
- ✅ 增量验证

### 第四阶段：成熟度与生态（部分）

- ✅ 多 PDK 支持（PdkRegistry）
- ✅ 并行 STA（rayon arc delay）
- ✅ 增量 STA
- ✅ 内存优化（Netlist::compact、Placement::write_to_file）
- ✅ 布线缓存（RoutingCache）
- ✅ 基准测试套件（ISCAS c17/c432、pipeline、NAND/MAJ chains）
- ✅ 性能基准（criterion：synthesis/placement/routing/timing）

---

## 待实现

| 优先级 | 功能 | 影响范围 |
|--------|------|---------|
| 低 | 用户指南 | 文档 |
| 低 | 跨平台 CI（Ubuntu/Windows/macOS 对称质量门） | 基础设施 |
| 低 | 自动发布（tag 触发 wheel + binary） | 分发 |
| 低 | API 版本化 | 稳定性 |

---

## 架构

```
输入格式: Verilog | BLIF | EDIF | SPICE | IR JSON | .bench
    ↓
综合 (rflux-synth): 布尔优化 → splitter/DFF 插入 → tech mapping
    ↓
布局 (rflux-place): LevelizedPlacer | SaPlacer | PartitionPlacer
    ↓
时钟树 (rflux-flow): H-tree + buffer sizing + skew optimization
    ↓
布线 (rflux-route): JTL/PTL 混合 A* + 拥塞/串扰感知
    ↓
时序 (rflux-timing): STA/SSTA + waveform-aware + OCV + 噪声裕度
    ↓
验证 (rflux-drc): DRC (6 rules) + LVS + SVG 可视化
    ↓
仿真 (rflux-sim): 事件驱动 + 内部 transient + 外部 JoSIM
    ↓
输出: GDS-II | SVG | JSON reports
```

## 对标工具

| rflux 模块 | 对标工具 | 关系 |
|------------|---------|------|
| rflux-ir, rflux-synth, rflux-verify | Yosys | SFQ 专用综合 + 等价检查 |
| rflux-place, rflux-route | OpenROAD / nextpnr | SFQ 物理实现原型 |
| rflux-timing | OpenSTA | SFQ 时序分析（含 waveform/OCV） |
| rflux-sim | JoSIM | SFQ 仿真（部分 JoSIM parity） |
| rflux-drc | 商业 DRC/LVS | SFQ 设计规则检查 |
