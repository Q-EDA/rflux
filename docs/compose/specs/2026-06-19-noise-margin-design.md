# SFQ 噪声裕度分析设计

日期：2026-06-19

## [S1] 概述

为 SFQ 电路添加噪声裕度分析，评估信号脉冲在噪声环境下的鲁棒性。综合考虑热噪声、串扰、工艺偏差和脉冲退化四个噪声源。

## [S2] 噪声裕度模型

### 信号幅度

从 WaveformPropagator 的 PulseEnvelope 获取经互连传播后的脉冲幅度（归一化 0~1）。经过逻辑门时幅度重置为 1.0（门有增益）。

### 噪声源

1. **热噪声**：σ_thermal = √(4kT × Δf) / V_pulse
   - k = 1.38e-23 J/K (玻尔兹曼常数)
   - T = 4.2K (SFQ 工作温度)
   - Δf ≈ 1/(π × t_pulse) (带宽)
   - V_pulse ≈ 2mV (SFQ 脉冲电压)

2. **串扰噪声**：从 CouplingMap 获取相邻线耦合系数
   - crosstalk_noise = coupling_coefficient × signal_amplitude

3. **工艺偏差**：Ic 变化导致阈值漂移
   - process_spread = OcvConfig 的 derating 偏差

4. **脉冲退化**：从 WaveformPropagator 的 degradation 检测
   - 当 amplitude < threshold 时，退化噪声 = threshold - amplitude

### 裕度计算

```rust
margin = signal_amplitude - noise_rms
margin_db = 20 * log10(signal_amplitude / noise_rms)  // SNR in dB
```

## [S3] 输出结构

```rust
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

pub struct NoiseMarginReport {
    pub nets: Vec<NetNoiseMargin>,
    pub worst_margin_db: f64,
    pub worst_net: Option<(PinRef, PinRef)>,
    pub violations: usize,
    pub temperature_k: f64,
}
```

## [S4] 配置

```rust
pub struct NoiseMarginConfig {
    pub temperature_k: f64,           // default 4.2K
    pub pulse_voltage_mv: f64,        // default 2.0mV
    pub pulse_width_ps: f64,          // default 1.0ps
    pub margin_threshold_db: f64,     // default 6.0dB (violation if below)
    pub enable_thermal: bool,         // default true
    pub enable_crosstalk: bool,       // default true
    pub enable_process_spread: bool,  // default true
}
```

## [S5] 集成点

1. `rflux-timing`：新增 `NoiseMarginAnalyzer`，在 `analyze()` 后执行
2. `TimingReport`：新增 `noise_margin: Option<NoiseMarginReport>` 字段
3. `TimingConfig`：新增 `noise_margin: NoiseMarginConfig` 字段
4. FlowConfig/CLI：`--noise-margin`、`--temperature` 参数

## [S6] 测试策略

- 热噪声在 4.2K 下应远小于信号幅度
- 无串扰时 margin 应接近信号幅度
- 高串扰应显著降低 margin
- 违例检测：margin_db < threshold 时 violations > 0
