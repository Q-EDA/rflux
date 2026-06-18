# Waveform-aware 时序 + OCV Derating 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 SFQ STA 添加脉冲包络传播和工艺偏差 derating

**Architecture:** 在 rflux-timing 中新增 WaveformPropagator 和 OcvDerater，扩展 TimingConfig/TimingArcReport，在 analyze() 中集成。

**Tech Stack:** Rust, rflux-timing, rflux-extract, rflux-tech

---

## 文件结构

- Modify: `crates/timing/src/lib.rs` (核心改动)
- Modify: `crates/flow/src/lib.rs` (FlowConfig 扩展)
- Modify: `crates/cli/src/main.rs` (CLI 参数)

---

## Task 1: 添加配置结构

**Covers:** [S3, S4]

**Files:**
- Modify: `crates/timing/src/lib.rs`

- [ ] **Step 1: 添加 PulseEnvelope 结构体**

在 `crates/timing/src/lib.rs` 中，在 `StatisticalTimingConfig` 之前添加：

```rust
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PulseEnvelope {
    pub arrival_ps: f64,
    pub amplitude: f64,
    pub width_ps: f64,
    pub rise_time_ps: f64,
}

impl PulseEnvelope {
    pub fn initial(amplitude: f64, width_ps: f64, rise_time_ps: f64) -> Self {
        Self {
            arrival_ps: 0.0,
            amplitude,
            width_ps,
            rise_time_ps,
        }
    }
}
```

- [ ] **Step 2: 添加 WaveformTimingConfig 结构体**

```rust
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WaveformTimingConfig {
    pub enable_waveform: bool,
    pub amplitude_threshold: f64,
    pub max_pulse_width_ps: f64,
    pub initial_amplitude: f64,
    pub initial_width_ps: f64,
    pub initial_rise_time_ps: f64,
}

impl Default for WaveformTimingConfig {
    fn default() -> Self {
        Self {
            enable_waveform: false,
            amplitude_threshold: 0.3,
            max_pulse_width_ps: 10.0,
            initial_amplitude: 1.0,
            initial_width_ps: 1.0,
            initial_rise_time_ps: 0.5,
        }
    }
}
```

- [ ] **Step 3: 添加 OcvConfig 结构体**

```rust
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OcvConfig {
    pub cell_early_factor: f64,
    pub cell_late_factor: f64,
    pub wire_early_factor: f64,
    pub wire_late_factor: f64,
    pub path_based: bool,
    pub path_depth_factor: f64,
}

impl Default for OcvConfig {
    fn default() -> Self {
        Self {
            cell_early_factor: 0.95,
            cell_late_factor: 1.05,
            wire_early_factor: 0.95,
            wire_late_factor: 1.05,
            path_based: false,
            path_depth_factor: 0.005,
        }
    }
}
```

- [ ] **Step 4: 扩展 TimingConfig**

在 `TimingConfig` 结构体中添加：

```rust
pub waveform: WaveformTimingConfig,
pub ocv: OcvConfig,
```

在 `Default` impl 中添加：

```rust
waveform: WaveformTimingConfig::default(),
ocv: OcvConfig::default(),
```

- [ ] **Step 5: 验证编译**

Run: `cargo check -p rflux-timing`
Expected: 编译通过（新字段有 Default，向后兼容）

- [ ] **Step 6: Commit**

```bash
git add crates/timing/src/lib.rs
git commit -m "feat(timing): add PulseEnvelope, WaveformTimingConfig, OcvConfig structs"
```

---

## Task 2: 扩展 TimingArcReport

**Covers:** [S5]

**Files:**
- Modify: `crates/timing/src/lib.rs`

- [ ] **Step 1: 添加字段到 TimingArcReport**

在 `TimingArcReport` 结构体中，在 `hold_slack_ps` 之后添加：

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub pulse_envelope: Option<PulseEnvelope>,
#[serde(default)]
pub pulse_degradation_violation: bool,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub ocv_early_arrival_ps: Option<f64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub ocv_late_arrival_ps: Option<f64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub ocv_early_slack_ps: Option<f64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub ocv_late_slack_ps: Option<f64>,
```

- [ ] **Step 2: 修复所有 TimingArcReport 构造点**

搜索代码中所有 `TimingArcReport {` 构造位置，添加新字段的默认值：

```rust
pulse_envelope: None,
pulse_degradation_violation: false,
ocv_early_arrival_ps: None,
ocv_late_arrival_ps: None,
ocv_early_slack_ps: None,
ocv_late_slack_ps: None,
```

注意：TimingArcReport 结构体的定义中已有 serde derives，所以这些新字段可以正常序列化。

- [ ] **Step 3: 运行测试**

Run: `cargo test -p rflux-timing`
Expected: 所有测试通过

- [ ] **Step 4: Commit**

```bash
git add crates/timing/src/lib.rs
git commit -m "feat(timing): extend TimingArcReport with waveform and OCV fields"
```

---

## Task 3: 实现 OCV Derater

**Covers:** [S3]

**Files:**
- Modify: `crates/timing/src/lib.rs`

- [ ] **Step 1: 添加 OcvDerater 结构体**

```rust
pub struct OcvDerater {
    config: OcvConfig,
}

impl OcvDerater {
    pub fn new(config: OcvConfig) -> Self {
        Self { config }
    }

    pub fn early_cell_factor(&self, depth: usize) -> f64 {
        let base = self.config.cell_early_factor;
        if self.config.path_based {
            (base - depth as f64 * self.config.path_depth_factor).max(0.8)
        } else {
            base
        }
    }

    pub fn late_cell_factor(&self, depth: usize) -> f64 {
        let base = self.config.cell_late_factor;
        if self.config.path_based {
            (base + depth as f64 * self.config.path_depth_factor).min(1.2)
        } else {
            base
        }
    }

    pub fn early_wire_factor(&self, depth: usize) -> f64 {
        let base = self.config.wire_early_factor;
        if self.config.path_based {
            (base - depth as f64 * self.config.path_depth_factor).max(0.8)
        } else {
            base
        }
    }

    pub fn late_wire_factor(&self, depth: usize) -> f64 {
        let base = self.config.wire_late_factor;
        if self.config.path_based {
            (base + depth as f64 * self.config.path_depth_factor).min(1.2)
        } else {
            base
        }
    }

    pub fn apply_early(&self, cell_delay: f64, wire_delay: f64, depth: usize) -> (f64, f64) {
        (
            cell_delay * self.early_cell_factor(depth),
            wire_delay * self.early_wire_factor(depth),
        )
    }

    pub fn apply_late(&self, cell_delay: f64, wire_delay: f64, depth: usize) -> (f64, f64) {
        (
            cell_delay * self.late_cell_factor(depth),
            wire_delay * self.late_wire_factor(depth),
        )
    }
}
```

- [ ] **Step 2: 添加单元测试**

```rust
#[test]
fn ocv_default_factors() {
    let derater = OcvDerater::new(OcvConfig::default());
    assert!((derater.early_cell_factor(0) - 0.95).abs() < 1e-6);
    assert!((derater.late_cell_factor(0) - 1.05).abs() < 1e-6);
}

#[test]
fn ocv_path_based_increases_with_depth() {
    let config = OcvConfig { path_based: true, path_depth_factor: 0.01, ..Default::default() };
    let derater = OcvDerater::new(config);
    assert!(derater.late_cell_factor(5) > derater.late_cell_factor(0));
    assert!(derater.early_cell_factor(5) < derater.early_cell_factor(0));
}

#[test]
fn ocv_factors_clamped() {
    let config = OcvConfig { path_based: true, path_depth_factor: 0.5, ..Default::default() };
    let derater = OcvDerater::new(config);
    assert!(derater.late_cell_factor(100) <= 1.2);
    assert!(derater.early_cell_factor(100) >= 0.8);
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p rflux-timing`
Expected: 所有测试通过

- [ ] **Step 4: Commit**

```bash
git add crates/timing/src/lib.rs
git commit -m "feat(timing): add OcvDerater with early/late/path-based derating"
```

---

## Task 4: 实现 WaveformPropagator

**Covers:** [S2]

**Files:**
- Modify: `crates/timing/src/lib.rs`

- [ ] **Step 1: 添加 WaveformPropagator 结构体**

```rust
pub struct WaveformPropagator {
    config: WaveformTimingConfig,
}

impl WaveformPropagator {
    pub fn new(config: WaveformTimingConfig) -> Self {
        Self { config }
    }

    pub fn initial_envelope(&self) -> PulseEnvelope {
        PulseEnvelope::initial(
            self.config.initial_amplitude,
            self.config.initial_width_ps,
            self.config.initial_rise_time_ps,
        )
    }

    pub fn propagate_through_wire(
        &self,
        input: &PulseEnvelope,
        r_per_um: f64,
        c_per_um: f64,
        l_per_um: f64,
        length_um: f64,
    ) -> PulseEnvelope {
        if length_um <= 0.0 {
            return *input;
        }

        let total_r = r_per_um * length_um;
        let total_c = c_per_um * length_um;

        // Amplitude: exp(-R/(2*Z0) * L) for transmission line
        // SFQ: R≈0, so amplitude mostly preserved; use small empirical decay
        let alpha = if total_r > 0.0 && l_per_um > 0.0 && c_per_um > 0.0 {
            let z0 = (l_per_um / c_per_um).sqrt();
            total_r / (2.0 * z0 * length_um)
        } else {
            0.001 // small empirical decay for SFQ
        };
        let amplitude = (input.amplitude * (-alpha * length_um).exp()).max(0.0);

        // Pulse width: W_out = W_in + k * sqrt(L*C), k ~ 0.5
        let lc_delay = if l_per_um > 0.0 && c_per_um > 0.0 {
            (l_per_um * c_per_um).sqrt() * length_um
        } else {
            0.0
        };
        let width = input.width_ps + 0.5 * lc_delay;

        // Rise time: sqrt(t_rise_in^2 + (2.2*R*C)^2)
        let rc_spread = 2.2 * total_r * total_c;
        let rise_time = (input.rise_time_ps.powi(2) + rc_spread.powi(2)).sqrt();

        // Delay from propagation
        let delay = if l_per_um > 0.0 && c_per_um > 0.0 {
            (l_per_um * c_per_um).sqrt() * length_um
        } else {
            0.0
        };

        PulseEnvelope {
            arrival_ps: input.arrival_ps + delay,
            amplitude,
            width_ps: width,
            rise_time_ps: rise_time,
        }
    }

    pub fn propagate_through_gate(&self, input: &PulseEnvelope, gate_delay_ps: f64) -> PulseEnvelope {
        // Gate regenerates pulse (SFQ gate has gain)
        PulseEnvelope {
            arrival_ps: input.arrival_ps + gate_delay_ps,
            amplitude: self.config.initial_amplitude, // reset
            width_ps: input.width_ps, // preserve
            rise_time_ps: input.rise_time_ps, // preserve
        }
    }

    pub fn is_degraded(&self, envelope: &PulseEnvelope) -> bool {
        envelope.amplitude < self.config.amplitude_threshold
            || envelope.width_ps > self.config.max_pulse_width_ps
    }
}
```

- [ ] **Step 2: 添加单元测试**

```rust
#[test]
fn waveform_initial_envelope() {
    let prop = WaveformPropagator::new(WaveformTimingConfig::default());
    let env = prop.initial_envelope();
    assert_eq!(env.amplitude, 1.0);
    assert_eq!(env.arrival_ps, 0.0);
}

#[test]
fn waveform_wire_preserves_amplitude_for_zero_r() {
    let prop = WaveformPropagator::new(WaveformTimingConfig::default());
    let env = prop.initial_envelope();
    let out = prop.propagate_through_wire(&env, 0.0, 0.1, 2.0, 100.0);
    // R=0 means no resistive loss
    assert!(out.amplitude > 0.9);
}

#[test]
fn waveform_wire_increases_width() {
    let prop = WaveformPropagator::new(WaveformTimingConfig::default());
    let env = prop.initial_envelope();
    let out = prop.propagate_through_wire(&env, 0.0, 0.1, 2.0, 100.0);
    assert!(out.width_ps > env.width_ps);
}

#[test]
fn waveform_gate_resets_amplitude() {
    let prop = WaveformPropagator::new(WaveformTimingConfig::default());
    let mut env = prop.initial_envelope();
    env.amplitude = 0.3;
    let out = prop.propagate_through_gate(&env, 8.0);
    assert_eq!(out.amplitude, 1.0);
}

#[test]
fn waveform_degradation_detection() {
    let config = WaveformTimingConfig { amplitude_threshold: 0.5, ..Default::default() };
    let prop = WaveformPropagator::new(config);
    let env = PulseEnvelope { arrival_ps: 0.0, amplitude: 0.2, width_ps: 1.0, rise_time_ps: 0.5 };
    assert!(prop.is_degraded(&env));
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p rflux-timing`
Expected: 所有测试通过

- [ ] **Step 4: Commit**

```bash
git add crates/timing/src/lib.rs
git commit -m "feat(timing): add WaveformPropagator for SFQ pulse envelope tracking"
```

---

## Task 5: 集成到 analyze() 方法

**Covers:** [S2, S5]

**Files:**
- Modify: `crates/timing/src/lib.rs`

- [ ] **Step 1: 在 analyze() 中集成 OCV 和 waveform**

找到 `StaticTimingAnalyzer::analyze()` 方法中的 arcs 构建循环（约 line 400-460）。在每个 arc 构建完成后，添加 OCV 和 waveform 后处理：

在 `analyze()` 方法中，在构建 `TimingArcReport` 之后、收集 arcs 之前，添加：

```rust
let ocv_derater = OcvDerater::new(config.ocv);
let waveform_propagator = if config.waveform.enable_waveform {
    Some(WaveformPropagator::new(config.waveform))
} else {
    None
};

// After building each arc:
let (ocv_early_cell, ocv_early_wire) = ocv_derater.apply_early(
    arc.cell_delay_ps, arc.wire_delay_ps, /* depth */ 0
);
let (ocv_late_cell, ocv_late_wire) = ocv_derater.apply_late(
    arc.cell_delay_ps, arc.wire_delay_ps, /* depth */ 0
);
arc.ocv_early_arrival_ps = Some(ocv_early_cell + ocv_early_wire);
arc.ocv_late_arrival_ps = Some(ocv_late_cell + ocv_late_wire);
arc.ocv_early_slack_ps = Some(arc.required_ps - arc.ocv_early_arrival_ps.unwrap());
arc.ocv_late_slack_ps = Some(arc.required_ps - arc.ocv_late_arrival_ps.unwrap());

if let Some(ref prop) = waveform_propagator {
    let mut envelope = prop.initial_envelope();
    // Propagate through cell
    envelope = prop.propagate_through_gate(&envelope, arc.cell_delay_ps);
    // Propagate through wire
    if let Some(route) = routing.routes.iter().find(|r| r.from == arc.from && r.to == arc.to) {
        // Use extracted parasitics if available, otherwise use defaults
        envelope = prop.propagate_through_wire(&envelope, 0.0, 0.1, 2.0, route.length_um);
    }
    envelope.arrival_ps = arc.arrival_ps;
    arc.pulse_degradation_violation = prop.is_degraded(&envelope);
    arc.pulse_envelope = Some(envelope);
}
```

注意：实际实现需要仔细处理 arcs 循环的位置。arcs 是在 forward/backward pass 之后构建的。需要在 arcs 构建循环中添加后处理。

- [ ] **Step 2: 运行测试**

Run: `cargo test -p rflux-timing`
Expected: 所有测试通过（waveform/OCV 默认关闭）

- [ ] **Step 3: Commit**

```bash
git add crates/timing/src/lib.rs
git commit -m "feat(timing): integrate waveform propagation and OCV derating into STA"
```

---

## Task 6: Flow + CLI 集成

**Covers:** [S4]

**Files:**
- Modify: `crates/flow/src/lib.rs`
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: FlowConfig 扩展**

在 `FlowConfig` 中添加（如果不存在）：

```rust
pub enable_waveform_timing: bool,
pub ocv_cell_late_factor: f64,
pub ocv_wire_late_factor: f64,
```

在 `Default` impl 中：

```rust
enable_waveform_timing: false,
ocv_cell_late_factor: 1.05,
ocv_wire_late_factor: 1.05,
```

在 `compile_layout` 中，将这些值传递给 `TimingConfig`。

- [ ] **Step 2: CLI 参数**

在 `LayoutCommandArgs` 中添加：

```rust
#[arg(long)]
enable_waveform: bool,
#[arg(long, default_value = "1.05")]
ocv_cell_late: f64,
#[arg(long, default_value = "1.05")]
ocv_wire_late: f64,
```

在命令处理中将这些值传递给 FlowConfig。

- [ ] **Step 3: 验证**

Run: `cargo check -p rflux-flow -p rflux-cli`

- [ ] **Step 4: Commit**

```bash
git add crates/flow/src/lib.rs crates/cli/src/main.rs
git commit -m "feat(flow,cli): add waveform and OCV timing options"
```

---

## Task 7: 最终验证

- [ ] **Step 1: 全量测试**

Run: `cargo test --workspace --exclude rflux-py`
Expected: 所有测试通过

- [ ] **Step 2: Clippy**

Run: `cargo clippy -p rflux-timing -p rflux-flow`
Expected: 无新增警告
