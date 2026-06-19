# SFQ 噪声裕度分析实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 SFQ 电路添加噪声裕度分析，综合热噪声、串扰、工艺偏差

**Architecture:** 在 rflux-timing 中新增 NoiseMarginAnalyzer，扩展 TimingReport/TimingConfig，集成到 flow/CLI。

**Tech Stack:** Rust, rflux-timing, rflux-route (CouplingMap), rflux-extract

---

## 文件结构

- Modify: `crates/timing/src/lib.rs` (核心实现)
- Modify: `crates/flow/src/lib.rs` (FlowConfig)
- Modify: `crates/cli/src/main.rs` (CLI 参数)

---

## Task 1: 添加配置和数据结构

**Covers:** [S3, S4]

**Files:**
- Modify: `crates/timing/src/lib.rs`

- [ ] **Step 1: 添加 NoiseMarginConfig**

在 OcvConfig 之后添加：

```rust
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NoiseMarginConfig {
    pub temperature_k: f64,
    pub pulse_voltage_mv: f64,
    pub pulse_width_ps: f64,
    pub margin_threshold_db: f64,
    pub enable_thermal: bool,
    pub enable_crosstalk: bool,
    pub enable_process_spread: bool,
}

impl Default for NoiseMarginConfig {
    fn default() -> Self {
        Self {
            temperature_k: 4.2,
            pulse_voltage_mv: 2.0,
            pulse_width_ps: 1.0,
            margin_threshold_db: 6.0,
            enable_thermal: true,
            enable_crosstalk: true,
            enable_process_spread: true,
        }
    }
}
```

- [ ] **Step 2: 添加 NetNoiseMargin 和 NoiseMarginReport**

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NetNoiseMargin {
    pub from: PinRef,
    pub to: PinRef,
    pub signal_amplitude: f64,
    pub noise_rms: f64,
    pub margin: f64,
    pub margin_db: f64,
    pub thermal_noise: f64,
    pub crosstalk_noise: f64,
    pub process_spread: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NoiseMarginReport {
    pub nets: Vec<NetNoiseMargin>,
    pub worst_margin_db: f64,
    pub worst_net: Option<(PinRef, PinRef)>,
    pub violations: usize,
    pub temperature_k: f64,
}
```

- [ ] **Step 3: 扩展 TimingConfig**

添加字段：

```rust
pub noise_margin: NoiseMarginConfig,
```

Default impl 中添加 `noise_margin: NoiseMarginConfig::default()`。

- [ ] **Step 4: 扩展 TimingReport**

添加字段：

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub noise_margin: Option<NoiseMarginReport>,
```

在所有 TimingReport 构造点添加 `noise_margin: None`。

- [ ] **Step 5: 验证编译**

Run: `cargo check -p rflux-timing`

- [ ] **Step 6: Commit**

```bash
git add crates/timing/src/lib.rs
git commit -m "feat(timing): add NoiseMarginConfig, NetNoiseMargin, NoiseMarginReport structs"
```

---

## Task 2: 实现 NoiseMarginAnalyzer

**Covers:** [S2]

**Files:**
- Modify: `crates/timing/src/lib.rs`

- [ ] **Step 1: 实现分析器**

```rust
pub struct NoiseMarginAnalyzer {
    config: NoiseMarginConfig,
}

impl NoiseMarginAnalyzer {
    pub fn new(config: NoiseMarginConfig) -> Self {
        Self { config }
    }

    pub fn analyze(
        &self,
        arcs: &[TimingArcReport],
        coupling_map: Option<&CouplingMap>,
        ocv_config: &OcvConfig,
    ) -> NoiseMarginReport {
        let mut nets = Vec::new();
        let k_boltzmann = 1.380649e-23; // J/K
        let t = self.config.temperature_k;
        let v_pulse = self.config.pulse_voltage_mv * 1e-3; // mV -> V
        let t_pulse = self.config.pulse_width_ps * 1e-12; // ps -> s
        let delta_f = 1.0 / (std::f64::consts::PI * t_pulse.max(1e-15));

        for arc in arcs {
            if arc.is_false_path {
                continue;
            }

            // Signal amplitude from pulse envelope or default
            let signal = arc.pulse_envelope
                .map(|e| e.amplitude)
                .unwrap_or(1.0);

            // Thermal noise (normalized to signal)
            let thermal = if self.config.enable_thermal && v_pulse > 0.0 {
                let v_thermal_rms = (4.0 * k_boltzmann * t * delta_f).sqrt();
                v_thermal_rms / v_pulse
            } else {
                0.0
            };

            // Crosstalk noise from CouplingMap
            let crosstalk = if self.config.enable_crosstalk {
                coupling_map
                    .map(|cm| cm.coupling_delay_ps(arc.from, arc.to).unwrap_or(0.0))
                    .unwrap_or(0.0)
                    / arc.wire_delay_ps.max(1.0) // normalize
                    * 0.1 // scale factor
            } else {
                0.0
            };

            // Process spread from OCV
            let process = if self.config.enable_process_spread {
                let late = ocv_config.cell_late_factor;
                let early = ocv_config.cell_early_factor;
                ((late - early) / 2.0).max(0.0)
            } else {
                0.0
            };

            let noise_rms = (thermal.powi(2) + crosstalk.powi(2) + process.powi(2)).sqrt();
            let margin = signal - noise_rms;
            let margin_db = if noise_rms > 0.0 && signal > 0.0 {
                20.0 * (signal / noise_rms).log10()
            } else {
                f64::INFINITY
            };

            nets.push(NetNoiseMargin {
                from: arc.from,
                to: arc.to,
                signal_amplitude: signal,
                noise_rms,
                margin,
                margin_db,
                thermal_noise: thermal,
                crosstalk_noise: crosstalk,
                process_spread: process,
            });
        }

        let violations = nets.iter()
            .filter(|n| n.margin_db < self.config.margin_threshold_db)
            .count();

        let (worst_margin_db, worst_net) = nets.iter()
            .map(|n| (n.margin_db, Some((n.from, n.to))))
            .fold((f64::INFINITY, None), |(best_m, best_n), (m, n)| {
                if m < best_m { (m, n) } else { (best_m, best_n) }
            });

        NoiseMarginReport {
            nets,
            worst_margin_db: if worst_margin_db.is_finite() { worst_margin_db } else { f64::INFINITY },
            worst_net,
            violations,
            temperature_k: t,
        }
    }
}
```

注意：需要检查 `CouplingMap` 是否有 `coupling_delay_ps(from, to)` 方法。如果没有，需要用 `coupling_score()` 或其他可用方法。查看 `crates/route/src/lib.rs` 中 CouplingMap 的 API。

- [ ] **Step 2: 添加单元测试**

```rust
#[test]
fn noise_margin_basic() {
    let config = NoiseMarginConfig::default();
    let analyzer = NoiseMarginAnalyzer::new(config);
    let arcs = vec![]; // empty
    let report = analyzer.analyze(&arcs, None, &OcvConfig::default());
    assert_eq!(report.nets.len(), 0);
    assert_eq!(report.violations, 0);
}

#[test]
fn noise_margin_thermal_small_at_4k() {
    let config = NoiseMarginConfig { temperature_k: 4.2, ..Default::default() };
    let analyzer = NoiseMarginAnalyzer::new(config);
    let arc = TimingArcReport {
        from: PinRef { node: NodeId(0), port: 0 },
        to: PinRef { node: NodeId(1), port: 0 },
        is_false_path: false,
        driver_kind: SfCellKind::GenericGate,
        route_mode: RouteMode::Jtl,
        route_length_um: 10.0,
        launch_phase: 0,
        capture_phase: 1,
        launch_window_start_ps: 0.0,
        launch_window_end_ps: 0.0,
        capture_window_start_ps: 0.0,
        capture_window_end_ps: 0.0,
        arrival_phase_offset_ps: 0.0,
        capture_window_slack_ps: 0.0,
        capture_window_violation: false,
        arrival_ps: 10.0,
        required_ps: 120.0,
        setup_slack_ps: 110.0,
        hold_slack_ps: 10.0,
        cell_delay_ps: 8.0,
        wire_delay_ps: 2.0,
        pulse_envelope: None,
        pulse_degradation_violation: false,
        ocv_early_arrival_ps: None,
        ocv_late_arrival_ps: None,
        ocv_early_slack_ps: None,
        ocv_late_slack_ps: None,
    };
    let report = analyzer.analyze(&[arc], None, &OcvConfig::default());
    assert_eq!(report.nets.len(), 1);
    // At 4.2K, thermal noise should be very small
    assert!(report.nets[0].thermal_noise < 0.01);
    assert!(report.nets[0].margin_db > 20.0); // good margin
}

#[test]
fn noise_margin_high_temperature_reduces_margin() {
    let config_low = NoiseMarginConfig { temperature_k: 4.2, ..Default::default() };
    let config_high = NoiseMarginConfig { temperature_k: 300.0, ..Default::default() };
    let analyzer_low = NoiseMarginAnalyzer::new(config_low);
    let analyzer_high = NoiseMarginAnalyzer::new(config_high);
    // Same arc
    // At 300K, thermal noise should be much larger
    // margin_high < margin_low
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p rflux-timing`
Expected: 所有测试通过

- [ ] **Step 4: Commit**

```bash
git add crates/timing/src/lib.rs
git commit -m "feat(timing): add NoiseMarginAnalyzer with thermal/crosstalk/process noise"
```

---

## Task 3: 集成到 analyze() 和 Flow/CLI

**Covers:** [S5]

**Files:**
- Modify: `crates/timing/src/lib.rs`
- Modify: `crates/flow/src/lib.rs`
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: 在 analyze() 中集成**

在 `StaticTimingAnalyzer::analyze()` 方法中，在构建 arcs 之后、返回 TimingReport 之前，添加：

```rust
let noise_margin_report = if config.noise_margin.enable_thermal
    || config.noise_margin.enable_crosstalk
    || config.noise_margin.enable_process_spread
{
    let analyzer = NoiseMarginAnalyzer::new(config.noise_margin);
    Some(analyzer.analyze(&arcs, coupling_map, &config.ocv))
} else {
    None
};
```

在 TimingReport 构造中设置 `noise_margin: noise_margin_report`。

- [ ] **Step 2: FlowConfig 扩展**

在 FlowConfig 中添加：

```rust
pub enable_noise_margin: bool,
pub noise_temperature_k: f64,
```

Default: `enable_noise_margin: false`, `noise_temperature_k: 4.2`

在 `compile_layout` 中传递给 TimingConfig。

- [ ] **Step 3: CLI 扩展**

在 LayoutCommandArgs 中添加：

```rust
#[arg(long)]
noise_margin: bool,
#[arg(long, default_value = "4.2")]
temperature_k: f64,
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p rflux-timing -p rflux-flow -p rflux-cli`
Expected: 所有测试通过

- [ ] **Step 5: Commit**

```bash
git add crates/timing/ crates/flow/ crates/cli/
git commit -m "feat(timing,flow,cli): integrate noise margin analysis with --noise-margin flag"
```

---

## Task 4: 最终验证

- [ ] **Step 1: 全量测试**

Run: `cargo test --workspace --exclude rflux-py`
Expected: 所有测试通过
