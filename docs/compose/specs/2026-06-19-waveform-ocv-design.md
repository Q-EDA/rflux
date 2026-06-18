# Waveform-aware 时序 + OCV Derating 设计

日期：2026-06-19

## [S1] 概述

为 SFQ 电路添加两项时序分析增强：

1. **Waveform-aware 时序**：追踪脉冲包络参数（幅度、宽度、上升时间）经过互连传播时的退化，替代纯标量 delay 模型
2. **OCV derating**：工艺偏差（JJ Ic、线宽等）的 early/late derating factor 模型

两者共享从寄生提取到时序修正的数据路径。

## [S2] 脉冲包络模型

```rust
pub struct PulseEnvelope {
    pub arrival_ps: f64,
    pub amplitude: f64,       // 归一化 0~1
    pub width_ps: f64,        // 脉冲宽度 (ps)
    pub rise_time_ps: f64,    // 上升时间 (ps)
}
```

传播规则（经过 RLC 互连）：
- 幅度：A_out = A_in × exp(-α×L)，α = R/(2×Z₀)（SFQ 中 R≈0，主要受反射影响）
- 脉冲宽度：W_out = W_in + k×√(L×C)
- 上升时间：t_rise_out = √(t_rise_in² + (2.2×R×C)²)

经过逻辑门时：
- 幅度重置为 1.0（门有增益）
- 宽度和上升时间受门本征延迟影响

## [S3] OCV Derating 模型

```rust
pub struct OcvConfig {
    pub cell_early_factor: f64,   // default 0.95
    pub cell_late_factor: f64,    // default 1.05
    pub wire_early_factor: f64,   // default 0.95
    pub wire_late_factor: f64,    // default 1.05
    pub path_based: bool,         // depth-based derating
    pub path_depth_factor: f64,   // 每级额外 derating (default 0.005)
}
```

Derating 应用：
- Setup 检查：launch path 用 late factor，capture path 用 early factor
- Hold 检查：launch path 用 early factor，capture path 用 late factor
- Path-based：factor = base_factor ± depth × depth_factor

## [S4] 配置结构

```rust
pub struct WaveformTimingConfig {
    pub enable_waveform: bool,
    pub amplitude_threshold: f64,    // default 0.3
    pub max_pulse_width_ps: f64,     // default 10.0
    pub initial_amplitude: f64,      // default 1.0
    pub initial_width_ps: f64,       // default 1.0
    pub initial_rise_time_ps: f64,   // default 0.5
}
```

扩展 `TimingConfig`：

```rust
pub struct TimingConfig {
    // ... existing fields ...
    pub waveform: WaveformTimingConfig,
    pub ocv: OcvConfig,
}
```

## [S5] TimingArcReport 扩展

```rust
pub struct TimingArcReport {
    // ... existing fields ...
    pub pulse_envelope: Option<PulseEnvelope>,    // waveform-aware
    pub pulse_degradation_violation: bool,         // 幅度低于阈值
    pub ocv_early_arrival_ps: Option<f64>,         // OCV early
    pub ocv_late_arrival_ps: Option<f64>,          // OCV late
    pub ocv_early_slack_ps: Option<f64>,           // OCV early slack
    pub ocv_late_slack_ps: Option<f64>,            // OCV late slack
}
```

## [S6] 实现路径

1. 在 `rflux-timing` 中新增 `WaveformPropagator` 和 `OcvDerater`
2. `analyze()` 方法中：先计算标量 delay → 应用 OCV → 传播波形包络
3. 输出报告包含 waveform 和 OCV 字段
4. FlowConfig/CLI 暴露配置选项

## [S7] 测试策略

- Waveform：100um PTL 后幅度应 > 0.5（无反射损耗时）
- OCV：late arrival > nominal > early arrival
- 组合：waveform + OCV 的 slack 应比纯标量更保守
- 回归：现有测试不应受影响（waveform/OCV 默认关闭）
