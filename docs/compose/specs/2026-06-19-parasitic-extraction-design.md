# SFQ 寄生提取设计

日期：2026-06-19

## [S1] 概述

为 rflux 添加从 layout 几何提取 R/C/L 寄生参数的能力，用于修正时序分析中的 wire delay。SFQ 使用超导互连，需要特殊处理动能电感 (L_kin) 和无直流电阻特性。

## [S2] 核心数据结构

### 新增 crate: `crates/extract/`

```rust
pub struct ParasiticConfig {
    pub trace_width_um: f64,
    pub trace_thickness_um: f64,
    pub dielectric_height_um: f64,
    pub london_depth_nm: f64,
    pub kinetic_inductance_per_um: f64,
}

pub struct ExtractedParasitics {
    pub r_per_um: f64,
    pub c_per_um: f64,
    pub l_per_um: f64,
    pub z0_ohm: f64,
    pub delay_ps_per_um: f64,
    pub total_length_um: f64,
    pub total_delay_ps: f64,
    pub total_capacitance_ff: f64,
    pub total_inductance_ph: f64,
}

pub struct NetParasitics {
    pub from: PinRef,
    pub to: PinRef,
    pub mode: RouteMode,
    pub parasitics: ExtractedParasitics,
}

pub struct ExtractionReport {
    pub nets: Vec<NetParasitics>,
    pub total_wire_delay_ps: f64,
    pub total_capacitance_ff: f64,
    pub total_inductance_ph: f64,
}
```

## [S3] 提取算法

### 闭式公式（快速模式）

对于 PTL（共面波导结构）：
- 几何电感：L_geo ≈ μ₀·acosh(h/(2w+t))/π
- 动能电感：L_kin = μ₀·λ_L²/(w·t) （λ_L 为 London 深度）
- 总电感：L = L_geo + L_kin
- 电容：C = ε₀·ε_r·w_eff/h （w_eff 为有效宽度）
- 特征阻抗：Z₀ = √(L/C)
- 传播延迟：t_pd = √(L·C)

对于 JTL（Josephson 传输线）：
- 使用 PDK 提供的标称值，按几何比例缩放

### 几何提取（精确模式）

- 从 CouplingMap 的空间分箱获取相邻线间距
- 计算互感 M = k·√(L₁·L₂)
- 耦合电容 C_cpl ≈ π·ε₀·ε_r·l_parallel / acosh(d/w)

## [S4] PDK 扩展

在 `Pdk` 结构体中新增可选字段：

```rust
pub struct Pdk {
    // ... existing fields ...
    pub material: Option<SfqMaterialParams>,
}

pub struct SfqMaterialParams {
    pub london_depth_nm: f64,
    pub trace_thickness_um: f64,
    pub dielectric_constant: f64,
    pub dielectric_height_um: f64,
    pub kinetic_inductance_ratio: f64, // L_kin / L_geo 比值
}
```

## [S5] 集成点

1. `rflux-tech`：PDK 新增 `SfqMaterialParams`
2. `rflux-extract`：新增 crate，实现 `ParasiticExtractor`
3. `rflux-timing`：`arc_delay_ps()` 可选使用提取结果
4. `rflux-flow`：routing 后自动执行提取
5. CLI：`extract-parasitics` 子命令
6. Python：`extract_parasitics()` 绑定

## [S6] 测试策略

- 闭式公式：与已知 CPW 解析解对比
- 集成测试：用 minimal PDK 跑完整 extract → STA 流程
- 回归测试：提取结果不应使现有测试时序恶化超过 10%
