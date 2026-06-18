# SFQ 寄生提取实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 从 layout 几何提取 R/C/L 寄生参数，修正 STA 中的 wire delay

**Architecture:** 新增 `crates/extract/` crate，实现基于传输线闭式公式的 SFQ 互连寄生提取。扩展 PDK 支持材料参数，集成到 timing 和 flow 中。

**Tech Stack:** Rust, rflux-tech, rflux-route, rflux-timing

---

## 文件结构

- Create: `crates/extract/Cargo.toml`
- Create: `crates/extract/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)
- Modify: `crates/tech/src/lib.rs` (SfqMaterialParams)
- Modify: `crates/timing/src/lib.rs` (使用提取结果)
- Modify: `crates/flow/Cargo.toml`
- Modify: `crates/flow/src/lib.rs` (集成提取)
- Modify: `crates/cli/Cargo.toml`
- Modify: `crates/cli/src/main.rs` (CLI 命令)

---

## Task 1: PDK 材料参数扩展

**Covers:** [S4]

**Files:**
- Modify: `crates/tech/src/lib.rs`

- [ ] **Step 1: 添加 SfqMaterialParams 结构体**

在 `crates/tech/src/lib.rs` 的 `Pdk` 结构体定义之前添加：

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SfqMaterialParams {
    pub london_depth_nm: f64,
    pub trace_thickness_um: f64,
    pub dielectric_constant: f64,
    pub dielectric_height_um: f64,
    pub kinetic_inductance_ratio: f64,
}

impl SfqMaterialParams {
    #[must_use]
    pub fn default_sfq5ee() -> Self {
        Self {
            london_depth_nm: 150.0,
            trace_thickness_um: 0.2,
            dielectric_constant: 4.0,
            dielectric_height_um: 1.0,
            kinetic_inductance_ratio: 1.0,
        }
    }
}
```

- [ ] **Step 2: 在 Pdk 中添加 material 字段**

在 `Pdk` 结构体中添加（`interconnect_timing` 字段之后）：

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub material: Option<SfqMaterialParams>,
```

- [ ] **Step 3: 在 Pdk::minimal() 中设置默认值**

在 `Pdk::minimal()` 的 `interconnect_timing` 字段赋值之后添加：

```rust
material: Some(SfqMaterialParams::default_sfq5ee()),
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p rflux-tech`
Expected: 所有测试通过（material 字段为 Option，serde default 保证向后兼容）

- [ ] **Step 5: Commit**

```bash
git add crates/tech/src/lib.rs
git commit -m "feat(tech): add SfqMaterialParams for parasitic extraction"
```

---

## Task 2: 创建 extract crate 骨架

**Covers:** [S2]

**Files:**
- Create: `crates/extract/Cargo.toml`
- Create: `crates/extract/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: workspace 添加 extract crate**

在 `Cargo.toml` 的 `members` 中添加 `"crates/extract"`。

- [ ] **Step 2: 创建 crates/extract/Cargo.toml**

```toml
[package]
name = "rflux-extract"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rflux-ir = { path = "../ir" }
rflux-tech = { path = "../tech" }
rflux-route = { path = "../route" }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror.workspace = true
```

- [ ] **Step 3: 创建 crates/extract/src/lib.rs**

```rust
use rflux_ir::PinRef;
use rflux_route::{NetRoute, RouteMode, RoutingReport};
use rflux_tech::{InterconnectKind, Pdk, SfqMaterialParams};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParasiticConfig {
    pub trace_width_um: f64,
    pub trace_thickness_um: f64,
    pub dielectric_height_um: f64,
    pub dielectric_constant: f64,
    pub london_depth_nm: f64,
    pub kinetic_inductance_ratio: f64,
}

impl Default for ParasiticConfig {
    fn default() -> Self {
        Self {
            trace_width_um: 1.0,
            trace_thickness_um: 0.2,
            dielectric_height_um: 1.0,
            dielectric_constant: 4.0,
            london_depth_nm: 150.0,
            kinetic_inductance_ratio: 1.0,
        }
    }
}

impl ParasiticConfig {
    pub fn from_pdk(pdk: &Pdk) -> Self {
        let mut config = Self::default();
        if let Some(ref mat) = pdk.material {
            config.trace_thickness_um = mat.trace_thickness_um;
            config.dielectric_height_um = mat.dielectric_height_um;
            config.dielectric_constant = mat.dielectric_constant;
            config.london_depth_nm = mat.london_depth_nm;
            config.kinetic_inductance_ratio = mat.kinetic_inductance_ratio;
        }
        config
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetParasitics {
    pub from: PinRef,
    pub to: PinRef,
    pub mode: RouteMode,
    pub parasitics: ExtractedParasitics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionReport {
    pub nets: Vec<NetParasitics>,
    pub total_wire_delay_ps: f64,
    pub total_capacitance_ff: f64,
    pub total_inductance_ph: f64,
}
```

- [ ] **Step 4: 验证编译**

Run: `cargo check -p rflux-extract`

---

## Task 3: 实现闭式提取算法

**Covers:** [S3]

**Files:**
- Modify: `crates/extract/src/lib.rs`

- [ ] **Step 1: 实现 ParasiticExtractor**

在 `crates/extract/src/lib.rs` 中添加：

```rust
pub struct ParasiticExtractor {
    config: ParasiticConfig,
}

impl ParasiticExtractor {
    pub fn new(config: ParasiticConfig) -> Self {
        Self { config }
    }

    pub fn from_pdk(pdk: &Pdk) -> Self {
        Self::new(ParasiticConfig::from_pdk(pdk))
    }

    pub fn extract_net(&self, route: &NetRoute) -> NetParasitics {
        let parasitics = match route.mode {
            RouteMode::Ptl => self.extract_ptl(route.length_um),
            RouteMode::Jtl => self.extract_jtl(route.length_um),
        };
        NetParasitics {
            from: route.from,
            to: route.to,
            mode: route.mode,
            parasitics,
        }
    }

    pub fn extract_report(&self, routing: &RoutingReport) -> ExtractionReport {
        let nets: Vec<NetParasitics> = routing
            .routes
            .iter()
            .map(|r| self.extract_net(r))
            .collect();
        let total_wire_delay_ps = nets.iter().map(|n| n.parasitics.total_delay_ps).sum();
        let total_capacitance_ff = nets.iter().map(|n| n.parasitics.total_capacitance_ff).sum();
        let total_inductance_ph = nets.iter().map(|n| n.parasitics.total_inductance_ph).sum();
        ExtractionReport {
            nets,
            total_wire_delay_ps,
            total_capacitance_ff,
            total_inductance_ph,
        }
    }

    fn extract_ptl(&self, length_um: f64) -> ExtractedParasitics {
        let w = self.config.trace_width_um;
        let t = self.config.trace_thickness_um;
        let h = self.config.dielectric_height_um;
        let eps_r = self.config.dielectric_constant;
        let lambda_l = self.config.london_depth_nm * 0.001; // nm -> um
        let kr = self.config.kinetic_inductance_ratio;

        let mu_0 = 4.0 * std::f64::consts::PI * 1e-7; // H/m
        let eps_0 = 8.854e-12; // F/m
        let c_light = 299_792_458.0; // m/s

        // 几何电感 (H/um -> pH/um)
        let l_geo_per_m = mu_0 * ((2.0 * h + w) / (w)).acosh().max(0.01);
        let l_geo_per_um = l_geo_per_m * 1e6 * 1e12; // H/m -> pH/um

        // 动能电感 (pH/um)
        let l_kin_per_um = if w > 0.0 && t > 0.0 && lambda_l > 0.0 {
            mu_0 * lambda_l * lambda_l / (w * t) * 1e6 * 1e12
        } else {
            0.0
        };

        // 总电感 = 几何电感 * (1 + kinetic_ratio)
        let l_per_um = l_geo_per_um * (1.0 + kr);

        // 电容 (F/um -> fF/um)
        let c_per_m = eps_0 * eps_r * w / h;
        let c_per_um = c_per_m * 1e6 * 1e15; // F/m -> fF/um

        // 特征阻抗
        let l_h_per_um = l_per_um * 1e-12; // pH -> H
        let c_f_per_um = c_per_um * 1e-15; // fF -> F
        let z0 = if c_f_per_um > 0.0 {
            (l_h_per_um / c_f_per_um).sqrt()
        } else {
            0.0
        };

        // 传播延迟 (ps/um)
        let delay_per_um = if c_f_per_um > 0.0 && l_h_per_um > 0.0 {
            (l_h_per_um * c_f_per_um).sqrt() * 1e12 // s -> ps
        } else {
            0.0
        };

        let r_per_um = 0.0; // 超导体 R≈0

        ExtractedParasitics {
            r_per_um,
            c_per_um,
            l_per_um,
            z0_ohm: z0,
            delay_ps_per_um: delay_per_um,
            total_length_um: length_um,
            total_delay_ps: delay_per_um * length_um,
            total_capacitance_ff: c_per_um * length_um,
            total_inductance_ph: l_per_um * length_um,
        }
    }

    fn extract_jtl(&self, length_um: f64) -> ExtractedParasitics {
        // JTL 使用 PDK 标称值
        let delay_per_um = 0.15; // ps/um，与 PDK default 一致
        let z0 = 2.0; // ohm
        let l_per_um = 2.0; // pH/um (估算)
        let c_per_um = l_per_um / (z0 * z0); // C = L/Z0²

        ExtractedParasitics {
            r_per_um: 0.0,
            c_per_um,
            l_per_um,
            z0_ohm: z0,
            delay_ps_per_um: delay_per_um,
            total_length_um: length_um,
            total_delay_ps: delay_per_um * length_um,
            total_capacitance_ff: c_per_um * length_um,
            total_inductance_ph: l_per_um * length_um,
        }
    }
}
```

- [ ] **Step 2: 添加单元测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ptl_parasitics_positive_values() {
        let config = ParasiticConfig::default();
        let extractor = ParasiticExtractor::new(config);
        let p = extractor.extract_ptl(100.0);
        assert!(p.c_per_um > 0.0);
        assert!(p.l_per_um > 0.0);
        assert!(p.z0_ohm > 0.0);
        assert!(p.delay_ps_per_um > 0.0);
        assert!(p.total_length_um == 100.0);
        assert!((p.total_delay_ps - p.delay_ps_per_um * 100.0).abs() < 1e-6);
    }

    #[test]
    fn jtl_parasitics_consistent_with_pdk() {
        let config = ParasiticConfig::default();
        let extractor = ParasiticExtractor::new(config);
        let p = extractor.extract_jtl(50.0);
        assert!((p.delay_ps_per_um - 0.15).abs() < 1e-6);
        assert!((p.z0_ohm - 2.0).abs() < 1e-6);
        assert!((p.total_delay_ps - 7.5).abs() < 1e-6);
    }

    #[test]
    fn ptl_z0_reasonable_range() {
        let config = ParasiticConfig::default();
        let extractor = ParasiticExtractor::new(config);
        let p = extractor.extract_ptl(1.0);
        // PTL Z0 应在 1-10 ohm 范围
        assert!(p.z0_ohm > 1.0 && p.z0_ohm < 10.0);
    }

    #[test]
    fn kinetic_inductance_increases_delay() {
        let mut config_low = ParasiticConfig::default();
        config_low.kinetic_inductance_ratio = 0.0;
        let mut config_high = ParasiticConfig::default();
        config_high.kinetic_inductance_ratio = 2.0;
        let ext_low = ParasiticExtractor::new(config_low);
        let ext_high = ParasiticExtractor::new(config_high);
        let p_low = ext_low.extract_ptl(100.0);
        let p_high = ext_high.extract_ptl(100.0);
        assert!(p_high.total_delay_ps > p_low.total_delay_ps);
    }

    #[test]
    fn extract_report_aggregates() {
        let config = ParasiticConfig::default();
        let extractor = ParasiticExtractor::new(config);
        let routing = RoutingReport {
            routes: vec![
                NetRoute {
                    from: PinRef::new(0, 0),
                    to: PinRef::new(1, 0),
                    mode: RouteMode::Ptl,
                    segments: vec![],
                    direct_length_um: 100.0,
                    length_um: 100.0,
                },
                NetRoute {
                    from: PinRef::new(1, 0),
                    to: PinRef::new(2, 0),
                    mode: RouteMode::Jtl,
                    segments: vec![],
                    direct_length_um: 50.0,
                    length_um: 50.0,
                },
            ],
            total_length_um: 150.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 1,
            ptl_routes: 1,
        };
        let report = extractor.extract_report(&routing);
        assert_eq!(report.nets.len(), 2);
        assert!(report.total_wire_delay_ps > 0.0);
    }
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p rflux-extract`
Expected: 所有测试通过

- [ ] **Step 4: Commit**

```bash
git add crates/extract/
git commit -m "feat(extract): implement SFQ parasitic extraction with closed-form CPW model"
```

---

## Task 4: 集成到 timing 引擎

**Covers:** [S5]

**Files:**
- Modify: `crates/timing/Cargo.toml`
- Modify: `crates/timing/src/lib.rs`

- [ ] **Step 1: 添加依赖**

在 `crates/timing/Cargo.toml` 中添加：

```toml
rflux-extract = { path = "../extract" }
```

- [ ] **Step 2: 在 TimingConfig 中添加使用提取的选项**

在 `TimingConfig` 结构体中添加：

```rust
#[serde(default)]
pub use_parasitic_extraction: bool,
```

在 `Default` 实现中添加：

```rust
use_parasitic_extraction: false,
```

- [ ] **Step 3: 修改 arc_delay_ps 函数**

找到 `arc_delay_ps` 函数（约 line 684），修改 wire delay 计算逻辑：

在函数开头添加提取逻辑：

```rust
let wire_delay_ps = if config.use_parasitic_extraction {
    let extractor = rflux_extract::ParasiticExtractor::from_pdk(pdk);
    let route = routing.route_for(from, to);
    match route {
        Some(route) => {
            let net_parasitics = extractor.extract_net(route);
            net_parasitics.parasitics.total_delay_ps
        }
        None => {
            // fallback 到查表
            pdk.interconnect_delay_ps(interconnect_kind(route_mode), route_length)
                .unwrap_or(0.0)
        }
    }
} else {
    // 原有查表逻辑
    pdk.interconnect_delay_ps(interconnect_kind(route_mode), route_length)
        .unwrap_or(0.0)
};
```

注意：需要检查 `routing` 是否有 `route_for(from, to)` 方法。如果没有，需要在 RoutingReport 上添加。

- [ ] **Step 4: 运行测试**

Run: `cargo test -p rflux-timing`
Expected: 所有测试通过（use_parasitic_extraction 默认 false，行为不变）

- [ ] **Step 5: Commit**

```bash
git add crates/timing/
git commit -m "feat(timing): integrate parasitic extraction into STA wire delay"
```

---

## Task 5: 集成到 flow 和 CLI

**Covers:** [S5]

**Files:**
- Modify: `crates/flow/Cargo.toml`
- Modify: `crates/flow/src/lib.rs`
- Modify: `crates/cli/Cargo.toml`
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: flow 添加依赖**

在 `crates/flow/Cargo.toml` 中添加：

```toml
rflux-extract = { path = "../extract" }
```

- [ ] **Step 2: 在 FlowConfig 中添加选项**

在 `FlowConfig` 结构体中添加：

```rust
#[serde(default)]
pub use_parasitic_extraction: bool,
```

在 `Default` 实现中设为 `false`。

- [ ] **Step 3: 在 compile_layout 中集成**

在 `compile_layout` 的 routing 完成后、timing 分析之前，添加提取逻辑：

```rust
let extraction_report = if config.use_parasitic_extraction {
    let extractor = rflux_extract::ParasiticExtractor::from_pdk(pdk);
    let report = extractor.extract_report(&artifacts.routing);
    Some(report)
} else {
    None
};
```

将 `config.use_parasitic_extraction` 传递给 timing config。

- [ ] **Step 4: CLI 添加 extract-parasitics 命令**

在 `Commands` 枚举中添加：

```rust
ExtractParasitics(ExtractParasiticsArgs),
```

添加参数结构体：

```rust
#[derive(Debug, Args)]
struct ExtractParasiticsArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, value_enum, default_value = "auto")]
    input_format: CliNetlistInputFormat,
    #[arg(long)]
    pdk: Option<PathBuf>,
    #[arg(long)]
    output: Option<PathBuf>,
}
```

实现命令处理：

```rust
fn run_extract_parasitics(args: &ExtractParasiticsArgs) -> Result<()> {
    let (mut netlist, pdk) = load_cli_netlist_and_pdk(&args.input, args.input_format, args.pdk.clone())?;
    let config = FlowConfig::default();
    let mut flow = FlowRunner::new();
    let layout_report = flow.compile_layout(&mut netlist, &pdk, &config)?;
    
    let extractor = rflux_extract::ParasiticExtractor::from_pdk(&pdk);
    // 需要从 flow 获取 routing report
    // 检查 FlowRunner 是否有方法获取 routing
    
    let output_json = serde_json::to_string_pretty(&report)?;
    if let Some(output_path) = &args.output {
        fs::write(output_path, &output_json)?;
    } else {
        println!("{}", output_json);
    }
    Ok(())
}
```

- [ ] **Step 5: 运行测试**

Run: `cargo test -p rflux-flow -p rflux-cli`
Expected: 所有测试通过

- [ ] **Step 6: Commit**

```bash
git add crates/flow/ crates/cli/
git commit -m "feat(flow,cli): integrate parasitic extraction into flow and add CLI command"
```

---

## Task 6: 最终验证

- [ ] **Step 1: 全量测试**

Run: `cargo test --workspace --exclude rflux-py`
Expected: 所有测试通过

- [ ] **Step 2: Clippy**

Run: `cargo clippy -p rflux-extract -p rflux-tech -p rflux-timing -p rflux-place`
Expected: 无警告
