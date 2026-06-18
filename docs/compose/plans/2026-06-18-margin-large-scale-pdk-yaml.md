# SFQ EDA 后端增强实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 rflux 补充三项独立能力：参数化裕度分析、大规模电路支持、PDK YAML 格式。

**Architecture:** 三个独立 track 并行开发。Track A 新增 `crates/margin/` crate；Track B 修改 `place`/`flow` crate 加入分区和并行；Track C 在 `tech`/`io`/`cli` 中加入 YAML 支持。

**Tech Stack:** Rust, serde_yaml, rayon, rand

---

## 文件结构

### Track A: 裕度分析
- Create: `crates/margin/Cargo.toml`
- Create: `crates/margin/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)
- Modify: `crates/cli/Cargo.toml`
- Modify: `crates/cli/src/main.rs` (新增 `analyze-margin` 子命令)
- Modify: `crates/py/Cargo.toml`
- Modify: `crates/py/src/lib.rs` (Python 绑定)

### Track B: 大规模电路
- Modify: `Cargo.toml` (workspace dependencies: rayon)
- Modify: `crates/place/Cargo.toml`
- Modify: `crates/place/src/lib.rs` (新增 `PartitionPlacer`)
- Modify: `crates/flow/Cargo.toml`
- Modify: `crates/flow/src/lib.rs` (DSE 并行化)

### Track C: PDK YAML
- Modify: `Cargo.toml` (workspace dependencies: serde_yaml)
- Modify: `crates/tech/Cargo.toml`
- Modify: `crates/tech/src/lib.rs` (from_yaml/to_yaml)
- Modify: `crates/io/Cargo.toml`
- Modify: `crates/io/src/lib.rs` (read_pdk_yaml, write_pdk_yaml)
- Modify: `crates/cli/Cargo.toml`
- Modify: `crates/cli/src/main.rs` (格式自动检测)

---

## Track C: PDK YAML 格式支持（最简单，优先实现）

### Task C1: 添加 serde_yaml 依赖

**Covers:** [S4]

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/tech/Cargo.toml`
- Modify: `crates/io/Cargo.toml`
- Modify: `crates/cli/Cargo.toml`

- [ ] **Step 1: workspace 根 Cargo.toml 添加 serde_yaml**

在 `[workspace.dependencies]` 中添加：

```toml
serde_yaml = "0.9"
```

- [ ] **Step 2: crates/tech/Cargo.toml 添加依赖**

```toml
[dependencies]
serde.workspace = true
serde_json.workspace = true
serde_yaml = { workspace = true, optional = true }

[features]
yaml = ["serde_yaml"]
default = []
```

- [ ] **Step 3: crates/io/Cargo.toml 添加依赖**

查看 `crates/io/Cargo.toml` 当前内容，在 `[dependencies]` 中添加：

```toml
serde_yaml = { workspace = true, optional = true }
```

同时在 `[features]` 中添加（如果已有 features 则追加）：

```toml
yaml = ["serde_yaml"]
```

- [ ] **Step 4: crates/cli/Cargo.toml 添加依赖**

```toml
serde_yaml.workspace = true
rflux-tech = { path = "../tech", features = ["yaml"] }
```

- [ ] **Step 5: 验证编译通过**

Run: `cargo check -p rflux-tech -p rflux-io -p rflux-cli`
Expected: 编译成功

### Task C2: Pdk 添加 YAML 序列化/反序列化

**Covers:** [S4]

**Files:**
- Modify: `crates/tech/src/lib.rs`

- [ ] **Step 1: 添加 from_yaml 和 to_yaml 方法**

在 `crates/tech/src/lib.rs` 的 `impl Pdk` 块中，在 `to_json` 方法之后添加：

```rust
#[cfg(feature = "yaml")]
pub fn from_yaml(serialized: &str) -> Result<Self, serde_yaml::Error> {
    serde_yaml::from_str(serialized)
}

#[cfg(feature = "yaml")]
pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
    serde_yaml::to_string(self)
}
```

- [ ] **Step 2: 添加 from_auto 自动检测方法**

在同一 impl 块中添加：

```rust
pub fn from_auto(content: &str, path: Option<&std::path::Path>) -> Result<Self, String> {
    if let Some(path) = path {
        match path.extension().and_then(|e| e.to_str()) {
            #[cfg(feature = "yaml")]
            Some("yaml" | "yml") => {
                return Self::from_yaml(content).map_err(|e| e.to_string())
            }
            Some("json") => {
                return Self::from_json(content).map_err(|e| e.to_string())
            }
            _ => {}
        }
    }
    Self::from_json(content).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: 验证编译通过**

Run: `cargo check -p rflux-tech --features yaml`
Expected: 编译成功

- [ ] **Step 4: 运行现有测试确保无回归**

Run: `cargo test -p rflux-tech`
Expected: 所有现有测试通过

### Task C3: rflux-io 添加 PDK YAML 读写

**Covers:** [S4]

**Files:**
- Modify: `crates/io/src/lib.rs`

- [ ] **Step 1: 添加 read_pdk_yaml 函数**

在 `read_pdk_json` 函数之后添加：

```rust
#[cfg(feature = "yaml")]
pub fn read_pdk_yaml(path: impl AsRef<Path>) -> Result<Pdk, IoError> {
    let content = fs::read_to_string(path)?;
    Pdk::from_yaml(&content).map_err(|e| IoError::Json(e.to_string()))
}

#[cfg(feature = "yaml")]
pub fn write_pdk_yaml(path: impl AsRef<Path>, pdk: &Pdk) -> Result<(), IoError> {
    let content = pdk.to_yaml().map_err(|e| IoError::Json(e.to_string()))?;
    fs::write(path, content)?;
    Ok(())
}
```

- [ ] **Step 2: 添加 read_pdk_auto 自动格式检测函数**

```rust
pub fn read_pdk_auto(path: impl AsRef<Path>) -> Result<Pdk, IoError> {
    let path_ref = path.as_ref();
    let content = fs::read_to_string(path_ref)?;
    Pdk::from_auto(&content, Some(path_ref))
        .map_err(|e| IoError::Json(e))
}
```

- [ ] **Step 3: 验证编译通过**

Run: `cargo check -p rflux-io --features yaml`
Expected: 编译成功

### Task C4: CLI 格式自动检测

**Covers:** [S4]

**Files:**
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: 修改 load_pdk 函数支持自动检测**

将 `load_pdk` 函数（line 3675-3681）改为：

```rust
fn load_pdk(path: Option<PathBuf>) -> Result<Pdk> {
    match path {
        Some(path) => {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("failed to read PDK from {}", path.display()))?;
            Pdk::from_auto(&content, Some(&path))
                .with_context(|| format!("failed to parse PDK from {}", path.display()))
        }
        None => Ok(Pdk::minimal("minimal-sfq")),
    }
}
```

- [ ] **Step 2: 修改 pdk-minimal 命令支持 --format 参数**

在 `PdkMinimalArgs` 结构体中添加：

```rust
#[arg(long, value_enum, default_value = "json")]
format: PdkOutputFormat,
```

添加枚举：

```rust
#[derive(Debug, Clone, Copy, ValueEnum)]
enum PdkOutputFormat {
    Json,
    Yaml,
}
```

修改 `pdk-minimal` 命令处理逻辑，根据 format 选择输出格式。

- [ ] **Step 3: 添加 YAML PDK 集成测试**

在 `crates/cli/src/main.rs` 的测试模块中添加：

```rust
#[test]
fn pdk_minimal_yaml_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.join("pdk.yaml");
    let cli = Cli::try_parse_from([
        "rflux", "pdk-minimal", "--format", "yaml",
        "--output", output_path.to_str().unwrap(),
    ]).unwrap();
    // 执行命令，验证输出是合法 YAML 且可反序列化
}
```

- [ ] **Step 4: 运行全部 CLI 测试**

Run: `cargo test -p rflux-cli`
Expected: 所有测试通过

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/tech/ crates/io/ crates/cli/
git commit -m "feat(tech,io,cli): add PDK YAML format support"
```

---

## Track A: 参数化裕度分析

### Task A1: 创建 margin crate 骨架

**Covers:** [S2]

**Files:**
- Create: `crates/margin/Cargo.toml`
- Create: `crates/margin/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: workspace 添加 margin crate**

在 `Cargo.toml` 的 `members` 中添加 `"crates/margin"`。

- [ ] **Step 2: 创建 crates/margin/Cargo.toml**

```toml
[package]
name = "rflux-margin"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rflux-ir = { path = "../ir" }
rflux-tech = { path = "../tech" }
rflux-timing = { path = "../timing" }
rflux-route = { path = "../route" }
serde = { workspace = true }
serde_json = { workspace = true }
rand = "0.8"
thiserror.workspace = true
```

- [ ] **Step 3: 创建 crates/margin/src/lib.rs 基础结构**

```rust
use rflux_ir::Netlist;
use rflux_route::RoutingReport;
use rflux_tech::Pdk;
use rflux_timing::{StaticTimingAnalyzer, TimingConfig, TimingReport};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MarginMethod {
    MonteCarlo { samples: usize },
    BoundarySweep { steps_per_param: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Distribution {
    Uniform,
    Normal { sigma_ratio: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarginParameter {
    pub name: String,
    pub nominal: f64,
    pub min: f64,
    pub max: f64,
    pub distribution: Distribution,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarginConfig {
    pub parameters: Vec<MarginParameter>,
    pub method: MarginMethod,
    pub seed: u64,
    pub clock_period_ps: f64,
}

impl Default for MarginConfig {
    fn default() -> Self {
        Self {
            parameters: Vec::new(),
            method: MarginMethod::MonteCarlo { samples: 1000 },
            seed: 42,
            clock_period_ps: 120.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarginSample {
    pub parameter_values: Vec<(String, f64)>,
    pub worst_setup_slack_ps: f64,
    pub worst_hold_slack_ps: f64,
    pub critical_path_delay_ps: f64,
    pub setup_violations: usize,
    pub hold_violations: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarginReport {
    pub method: String,
    pub total_samples: usize,
    pub passed_samples: usize,
    pub yield_estimate: f64,
    pub worst_setup_slack_ps: f64,
    pub worst_hold_slack_ps: f64,
    pub sensitivity: Vec<(String, f64)>,
    pub worst_case_parameters: Vec<(String, f64)>,
    pub samples: Vec<MarginSample>,
}
```

- [ ] **Step 4: 验证编译通过**

Run: `cargo check -p rflux-margin`
Expected: 编译成功

### Task A2: 实现参数采样引擎

**Covers:** [S2]

**Files:**
- Modify: `crates/margin/src/lib.rs`

- [ ] **Step 1: 实现 MonteCarlo 采样函数**

```rust
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

fn sample_parameter(rng: &mut StdRng, param: &MarginParameter) -> f64 {
    match param.distribution {
        Distribution::Uniform => rng.gen_range(param.min..=param.max),
        Distribution::Normal { sigma_ratio } => {
            let sigma = (param.max - param.min) * sigma_ratio / 2.0;
            let mean = param.nominal;
            // Box-Muller transform
            let u1: f64 = rng.gen_range(0.0001..=1.0);
            let u2: f64 = rng.gen_range(0.0..=1.0);
            let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            (mean + z * sigma).clamp(param.min, param.max)
        }
    }
}

fn generate_mc_samples(
    config: &MarginConfig,
    rng: &mut StdRng,
) -> Vec<Vec<(String, f64)>> {
    let n = match config.method {
        MarginMethod::MonteCarlo { samples } => samples,
        _ => return vec![],
    };
    (0..n)
        .map(|_| {
            config
                .parameters
                .iter()
                .map(|p| (p.name.clone(), sample_parameter(rng, p)))
                .collect()
        })
        .collect()
}

fn generate_boundary_samples(config: &MarginConfig) -> Vec<Vec<(String, f64)>> {
    let steps = match config.method {
        MarginMethod::BoundarySweep { steps_per_param } => steps_per_param,
        _ => return vec![],
    };
    if config.parameters.is_empty() {
        return vec![];
    }
    // Generate all combinations of parameter values
    let param_values: Vec<Vec<f64>> = config
        .parameters
        .iter()
        .map(|p| {
            (0..=steps)
                .map(|i| {
                    let t = i as f64 / steps as f64;
                    p.min + t * (p.max - p.min)
                })
                .collect()
        })
        .collect();

    fn cartesian_product(acc: Vec<Vec<f64>>, remaining: &[Vec<f64>]) -> Vec<Vec<f64>> {
        if remaining.is_empty() {
            return acc;
        }
        let mut result = Vec::new();
        for combo in &acc {
            for &val in &remaining[0] {
                let mut new_combo = combo.clone();
                new_combo.push(val);
                result.push(new_combo);
            }
        }
        cartesian_product(result, &remaining[1..])
    }

    let combos = cartesian_product(vec![vec![]], &param_values);
    combos
        .into_iter()
        .map(|combo| {
            config
                .parameters
                .iter()
                .zip(combo.iter())
                .map(|(p, &v)| (p.name.clone(), v))
                .collect()
        })
        .collect()
}
```

- [ ] **Step 2: 添加采样单元测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uniform_sampling_within_bounds() {
        let param = MarginParameter {
            name: "ic".to_string(),
            nominal: 1.0,
            min: 0.9,
            max: 1.1,
            distribution: Distribution::Uniform,
        };
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..1000 {
            let val = sample_parameter(&mut rng, &param);
            assert!(val >= 0.9 && val <= 1.1);
        }
    }

    #[test]
    fn test_boundary_combinations_count() {
        let config = MarginConfig {
            parameters: vec![
                MarginParameter {
                    name: "a".to_string(),
                    nominal: 1.0,
                    min: 0.0,
                    max: 2.0,
                    distribution: Distribution::Uniform,
                },
                MarginParameter {
                    name: "b".to_string(),
                    nominal: 1.0,
                    min: 0.0,
                    max: 2.0,
                    distribution: Distribution::Uniform,
                },
            ],
            method: MarginMethod::BoundarySweep { steps_per_param: 3 },
            seed: 42,
            clock_period_ps: 120.0,
        };
        let samples = generate_boundary_samples(&config);
        // 2 params, 4 values each (0,1,2,3 steps) => 4*4 = 16
        assert_eq!(samples.len(), 16);
    }
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p rflux-margin`
Expected: 测试通过

### Task A3: 实现裕度分析引擎

**Covers:** [S2]

**Files:**
- Modify: `crates/margin/src/lib.rs`

- [ ] **Step 1: 实现 PDK 参数修改函数**

```rust
fn apply_parameters_to_pdk(base: &Pdk, params: &[(String, f64)]) -> Pdk {
    let mut pdk = base.clone();
    for (name, value) in params {
        match name.as_str() {
            "jtl_impedance_ohm" => pdk.jtl_impedance_ohm = *value,
            "ptl_impedance_ohm" => pdk.ptl_impedance_ohm = *value,
            "jtl_propagation_delay_ps_per_um" => pdk.jtl_propagation_delay_ps_per_um = *value,
            "ptl_propagation_delay_ps_per_um" => pdk.ptl_propagation_delay_ps_per_um = *value,
            other => {
                // Check if it's a cell timing parameter like "cell_timing.GenericGate.intrinsic_delay_ps"
                if let Some((kind_str, field)) = other.strip_prefix("cell_timing.").and_then(|s| s.rsplit_once('.')) {
                    if let Ok(kind) = serde_json::from_str::<rflux_tech::SfCellKind>(&format!("\"{}\"", kind_str)) {
                        if let Some(timing) = pdk.cell_timing.iter_mut().find(|t| t.kind == kind) {
                            match field {
                                "intrinsic_delay_ps" => timing.intrinsic_delay_ps = *value,
                                "setup_ps" => timing.setup_ps = *value,
                                "hold_ps" => timing.hold_ps = *value,
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }
    pdk
}
```

- [ ] **Step 2: 实现敏感度计算**

```rust
fn compute_sensitivity(
    samples: &[MarginSample],
    param_names: &[String],
) -> Vec<(String, f64)> {
    if samples.is_empty() || param_names.is_empty() {
        return Vec::new();
    }

    param_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            // Pearson correlation between parameter i and worst_setup_slack
            let values: Vec<f64> = samples.iter().map(|s| s.parameter_values[i].1).collect();
            let slacks: Vec<f64> = samples.iter().map(|s| s.worst_setup_slack_ps).collect();
            let n = values.len() as f64;
            let mean_v: f64 = values.iter().sum::<f64>() / n;
            let mean_s: f64 = slacks.iter().sum::<f64>() / n;
            let cov: f64 = values
                .iter()
                .zip(slacks.iter())
                .map(|(v, s)| (v - mean_v) * (s - mean_s))
                .sum::<f64>()
                / n;
            let std_v = (values.iter().map(|v| (v - mean_v).powi(2)).sum::<f64>() / n).sqrt();
            let std_s = (slacks.iter().map(|s| (s - mean_s).powi(2)).sum::<f64>() / n).sqrt();
            let correlation = if std_v > 0.0 && std_s > 0.0 {
                cov / (std_v * std_s)
            } else {
                0.0
            };
            (name.clone(), correlation)
        })
        .collect()
}
```

- [ ] **Step 3: 实现 analyze_margin 主函数**

```rust
pub fn analyze_margin(
    netlist: &Netlist,
    routing: &RoutingReport,
    pdk: &Pdk,
    config: &MarginConfig,
) -> MarginReport {
    let mut rng = StdRng::seed_from_u64(config.seed);
    let param_sets = match config.method {
        MarginMethod::MonteCarlo { .. } => generate_mc_samples(config, &mut rng),
        MarginMethod::BoundarySweep { .. } => generate_boundary_samples(config),
    };

    let analyzer = StaticTimingAnalyzer::new();
    let timing_config = TimingConfig {
        clock_period_ps: config.clock_period_ps,
        ..Default::default()
    };

    let mut samples = Vec::new();
    for params in &param_sets {
        let modified_pdk = apply_parameters_to_pdk(pdk, params);
        let report = analyzer.analyze(netlist, routing, &modified_pdk, &timing_config);
        match report {
            Ok(report) => {
                samples.push(MarginSample {
                    parameter_values: params.clone(),
                    worst_setup_slack_ps: report.worst_setup_slack_ps,
                    worst_hold_slack_ps: report.worst_hold_slack_ps,
                    critical_path_delay_ps: report.critical_path_delay_ps,
                    setup_violations: report.setup_violations,
                    hold_violations: report.hold_violations,
                });
            }
            Err(_) => {
                // Treat analysis failure as worst-case
                samples.push(MarginSample {
                    parameter_values: params.clone(),
                    worst_setup_slack_ps: f64::NEG_INFINITY,
                    worst_hold_slack_ps: f64::NEG_INFINITY,
                    critical_path_delay_ps: f64::INFINITY,
                    setup_violations: usize::MAX,
                    hold_violations: usize::MAX,
                });
            }
        }
    }

    let passed = samples
        .iter()
        .filter(|s| s.setup_violations == 0 && s.hold_violations == 0)
        .count();
    let total = samples.len();
    let yield_estimate = if total > 0 {
        passed as f64 / total as f64
    } else {
        0.0
    };

    let worst_setup = samples
        .iter()
        .map(|s| s.worst_setup_slack_ps)
        .fold(f64::INFINITY, f64::min);
    let worst_hold = samples
        .iter()
        .map(|s| s.worst_hold_slack_ps)
        .fold(f64::INFINITY, f64::min);

    let sensitivity = compute_sensitivity(&samples, &config.parameters.iter().map(|p| p.name.clone()).collect::<Vec<_>>());

    let worst_idx = samples
        .iter()
        .enumerate()
        .min_by(|a, b| {
            a.1.worst_setup_slack_ps
                .partial_cmp(&b.1.worst_setup_slack_ps)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i);
    let worst_case_parameters = worst_idx
        .map(|i| samples[i].parameter_values.clone())
        .unwrap_or_default();

    let method_str = match config.method {
        MarginMethod::MonteCarlo { samples } => format!("monte_carlo({})", samples),
        MarginMethod::BoundarySweep { steps_per_param } => {
            format!("boundary_sweep({})", steps_per_param)
        }
    };

    MarginReport {
        method: method_str,
        total_samples: total,
        passed_samples: passed,
        yield_estimate,
        worst_setup_slack_ps: worst_setup,
        worst_hold_slack_ps: worst_hold,
        sensitivity,
        worst_case_parameters,
        samples,
    }
}
```

- [ ] **Step 4: 添加集成测试**

```rust
#[test]
fn analyze_margin_minimal_circuit() {
    use rflux_ir::Netlist;
    use rflux_route::RoutingReport;
    use rflux_tech::Pdk;

    let netlist = Netlist::new();
    let routing = RoutingReport::default();
    let pdk = Pdk::minimal("test");
    let config = MarginConfig {
        parameters: vec![
            MarginParameter {
                name: "jtl_impedance_ohm".to_string(),
                nominal: 2.0,
                min: 1.8,
                max: 2.2,
                distribution: Distribution::Uniform,
            },
        ],
        method: MarginMethod::MonteCarlo { samples: 50 },
        seed: 42,
        clock_period_ps: 120.0,
    };
    let report = analyze_margin(&netlist, &routing, &pdk, &config);
    assert_eq!(report.total_samples, 50);
    assert!(report.yield_estimate >= 0.0 && report.yield_estimate <= 1.0);
}
```

- [ ] **Step 5: 运行测试**

Run: `cargo test -p rflux-margin`
Expected: 测试通过

- [ ] **Step 6: Commit**

```bash
git add crates/margin/ Cargo.toml
git commit -m "feat(margin): add parameterized margin analysis crate"
```

### Task A4: CLI 集成

**Covers:** [S2, S5]

**Files:**
- Modify: `crates/cli/Cargo.toml`
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: cli Cargo.toml 添加 margin 依赖**

```toml
rflux-margin = { path = "../margin" }
```

- [ ] **Step 2: 添加 AnalyzeMarginArgs 结构体和子命令**

在 `Commands` 枚举中添加：

```rust
AnalyzeMargin(AnalyzeMarginArgs),
```

添加参数结构体：

```rust
#[derive(Debug, Args)]
struct AnalyzeMarginArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, value_enum, default_value = "auto")]
    input_format: CliNetlistInputFormat,
    #[arg(long)]
    pdk: Option<PathBuf>,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "monte-carlo")]
    method: MarginMethodCli,
    #[arg(long, default_value = "1000")]
    samples: usize,
    #[arg(long, default_value = "3")]
    steps: usize,
    #[arg(long, default_value = "42")]
    seed: u64,
    #[arg(long)]
    clock_period_ps: Option<f64>,
    #[arg(long)]
    param: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MarginMethodCli {
    MonteCarlo,
    BoundarySweep,
}
```

- [ ] **Step 3: 实现 analyze-margin 命令处理**

```rust
fn run_analyze_margin(args: &AnalyzeMarginArgs) -> Result<()> {
    let (mut netlist, pdk) = load_cli_netlist_and_pdk(&args.input, args.input_format, args.pdk.clone())?;

    // Parse --param arguments: "name,min,max,distribution"
    let parameters: Vec<MarginParameter> = args
        .param
        .iter()
        .map(|p| {
            let parts: Vec<&str> = p.split(',').collect();
            if parts.len() < 4 {
                anyhow::bail!("--param format: name,min,max,uniform|normal");
            }
            let distribution = match parts[3] {
                "uniform" => Distribution::Uniform,
                "normal" => Distribution::Normal { sigma_ratio: 0.15 },
                _ => anyhow::bail!("distribution must be 'uniform' or 'normal'"),
            };
            Ok(MarginParameter {
                name: parts[0].to_string(),
                nominal: 0.0,
                min: parts[1].parse()?,
                max: parts[2].parse()?,
                distribution,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let method = match args.method {
        MarginMethodCli::MonteCarlo => MarginMethod::MonteCarlo { samples: args.samples },
        MarginMethodCli::BoundarySweep => MarginMethod::BoundarySweep { steps_per_param: args.steps },
    };

    let clock_period_ps = args.clock_period_ps.unwrap_or(120.0);

    // Need routing - run a minimal flow to get routing report
    let config = FlowConfig {
        timing: rflux_timing::TimingConfig {
            clock_period_ps,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut flow = FlowRunner::new();
    let layout_report = flow.compile_layout(&mut netlist, &pdk, &config)?;

    let margin_config = MarginConfig {
        parameters,
        method,
        seed: args.seed,
        clock_period_ps,
    };

    let report = rflux_margin::analyze_margin(
        &netlist,
        &layout_report.routing_detail(),  // 需要检查实际 API
        &pdk,
        &margin_config,
    );

    let output_json = serde_json::to_string_pretty(&report)?;
    if let Some(output_path) = &args.output {
        fs::write(output_path, &output_json)?;
    } else {
        println!("{}", output_json);
    }
    Ok(())
}
```

- [ ] **Step 4: 在命令分发中添加**

在 `main()` 的 match 中添加：

```rust
Commands::AnalyzeMargin(args) => run_analyze_margin(&args),
```

- [ ] **Step 5: 运行测试**

Run: `cargo test -p rflux-cli`
Expected: 所有测试通过

### Task A5: Python 绑定

**Covers:** [S2, S5]

**Files:**
- Modify: `crates/py/Cargo.toml`
- Modify: `crates/py/src/lib.rs`

- [ ] **Step 1: py Cargo.toml 添加依赖**

```toml
rflux-margin = { path = "../margin" }
```

- [ ] **Step 2: 添加 Python 函数**

在 `crates/py/src/lib.rs` 中添加：

```rust
#[pyfunction]
fn analyze_margin(
    netlist: &PyCircuit,
    pdk: &PyPdk,
    routing: &PyRoutingReport,
    method: &str,
    samples: usize,
    seed: u64,
    clock_period_ps: f64,
    parameters: Vec<(String, f64, f64, String)>,
) -> PyResult<PyMarginReport> {
    let params: Vec<rflux_margin::MarginParameter> = parameters
        .into_iter()
        .map(|(name, min, max, dist)| {
            let distribution = match dist.as_str() {
                "uniform" => rflux_margin::Distribution::Uniform,
                "normal" => rflux_margin::Distribution::Normal { sigma_ratio: 0.15 },
                _ => rflux_margin::Distribution::Uniform,
            };
            rflux_margin::MarginParameter {
                name,
                nominal: 0.0,
                min,
                max,
                distribution,
            }
        })
        .collect();

    let margin_method = match method {
        "monte_carlo" => rflux_margin::MarginMethod::MonteCarlo { samples },
        "boundary_sweep" => rflux_margin::MarginMethod::BoundarySweep { steps_per_param: samples },
        _ => rflux_margin::MarginMethod::MonteCarlo { samples },
    };

    let config = rflux_margin::MarginConfig {
        parameters: params,
        method: margin_method,
        seed,
        clock_period_ps,
    };

    let report = rflux_margin::analyze_margin(
        &netlist.inner,
        &routing.inner,
        &pdk.inner,
        &config,
    );

    Ok(PyMarginReport { inner: report })
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p rflux-py`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add crates/margin/ crates/cli/ crates/py/
git commit -m "feat: add margin analysis CLI and Python bindings"
```

---

## Track B: 大规模电路支持

### Task B1: 添加 rayon 依赖

**Covers:** [S3]

**Files:**
- Modify: `Cargo.toml` (workspace)
- Modify: `crates/flow/Cargo.toml`

- [ ] **Step 1: workspace 添加 rayon**

在 `Cargo.toml` 的 `[workspace.dependencies]` 中添加：

```toml
rayon = "1.10"
```

- [ ] **Step 2: crates/flow/Cargo.toml 添加依赖**

```toml
rayon.workspace = true
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p rflux-flow`
Expected: 编译成功

### Task B2: 实现分区布局器

**Covers:** [S3]

**Files:**
- Modify: `crates/place/src/lib.rs`

- [ ] **Step 1: 添加 PartitionConfig**

在 `PlacementConfig` 之后添加：

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionConfig {
    pub max_partition_size: usize,
    pub overlap_margin_um: f64,
    pub enable_partitioning: bool,
}

impl Default for PartitionConfig {
    fn default() -> Self {
        Self {
            max_partition_size: 500,
            overlap_margin_um: 20.0,
            enable_partitioning: false,
        }
    }
}
```

- [ ] **Step 2: 实现分区逻辑**

```rust
pub struct PartitionPlacer {
    config: PlacementConfig,
    partition_config: PartitionConfig,
}

impl PartitionPlacer {
    pub fn new(config: PlacementConfig, partition_config: PartitionConfig) -> Self {
        Self { config, partition_config }
    }

    pub fn place(&self, netlist: &Netlist) -> Result<Placement, PlaceError> {
        if !self.partition_config.enable_partitioning {
            let placer = LevelizedPlacer::new(self.config.clone());
            return placer.place(netlist);
        }

        let levels = topological_levels(netlist)?;
        let max_size = self.partition_config.max_partition_size;

        // Check if any level exceeds max_partition_size
        let needs_partitioning = levels.iter().any(|(_, nodes)| nodes.len() > max_size);

        if !needs_partitioning {
            let placer = LevelizedPlacer::new(self.config.clone());
            return placer.place(netlist);
        }

        // Split oversized levels into partitions
        let mut partitions: Vec<Vec<NodeId>> = Vec::new();
        for (_, mut level_nodes) in levels {
            while level_nodes.len() > max_size {
                let partition: Vec<NodeId> = level_nodes.drain(..max_size).collect();
                partitions.push(partition);
            }
            if !level_nodes.is_empty() {
                partitions.push(level_nodes);
            }
        }

        // Place each partition independently
        let mut all_placed = Vec::new();
        let mut y_offset = 0.0f64;

        for partition in &partitions {
            let sub_netlist = extract_subgraph(netlist, partition);
            let placer = LevelizedPlacer::new(self.config.clone());
            let sub_placement = placer.place(&sub_netlist)?;

            for mut placed in sub_placement.nodes {
                placed.point.y_um += y_offset;
                all_placed.push(placed);
            }
            y_offset += sub_placement.height_um + self.partition_config.overlap_margin_um;
        }

        let width = all_placed
            .iter()
            .map(|p| p.point.x_um)
            .fold(0.0f64, f64::max)
            + self.config.x_pitch_um;

        Ok(Placement {
            nodes: all_placed,
            width_um: width,
            height_um: y_offset,
        })
    }
}

fn topological_levels(netlist: &Netlist) -> Result<Vec<(usize, Vec<NodeId>)>, PlaceError> {
    // Use the same topological sort as LevelizedPlacer
    let mut in_degree: BTreeMap<NodeId, usize> = BTreeMap::new();
    let mut adjacency: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();

    for node in netlist.nodes() {
        in_degree.entry(node.id).or_insert(0);
        adjacency.entry(node.id).or_default();
    }

    for edge in netlist.edges() {
        *in_degree.entry(edge.to_node).or_insert(0) += 1;
        adjacency.entry(edge.from_node).or_default().push(edge.to_node);
    }

    let mut queue: VecDeque<NodeId> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut levels: BTreeMap<usize, Vec<NodeId>> = BTreeMap::new();
    let mut node_level: BTreeMap<NodeId, usize> = BTreeMap::new();

    while let Some(node) = queue.pop_front() {
        let level = node_level.get(&node).copied().unwrap_or(0);
        levels.entry(level).or_default().push(node);

        if let Some(successors) = adjacency.get(&node) {
            for &succ in successors {
                let new_level = level + 1;
                node_level
                    .entry(succ)
                    .and_modify(|l| *l = (*l).max(new_level))
                    .or_insert(new_level);

                if let Some(deg) = in_degree.get_mut(&succ) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(succ);
                    }
                }
            }
        }
    }

    Ok(levels.into_iter().collect())
}

fn extract_subgraph(netlist: &Netlist, nodes: &[NodeId]) -> Netlist {
    // Create a sub-netlist containing only the specified nodes
    let node_set: BTreeSet<NodeId> = nodes.iter().copied().collect();
    let mut sub = Netlist::new();

    for node in netlist.nodes() {
        if node_set.contains(&node.id) {
            sub.add_node(node.clone());
        }
    }

    for edge in netlist.edges() {
        if node_set.contains(&edge.from_node) && node_set.contains(&edge.to_node) {
            sub.add_edge(edge.clone());
        }
    }

    sub
}
```

- [ ] **Step 3: 添加分区测试**

```rust
#[test]
fn partition_placer_small_circuit_no_partition() {
    let netlist = create_test_netlist(10);
    let placer = PartitionPlacer::new(
        PlacementConfig::default(),
        PartitionConfig {
            max_partition_size: 500,
            enable_partitioning: true,
            ..Default::default()
        },
    );
    let placement = placer.place(&netlist).unwrap();
    assert_eq!(placement.nodes.len(), 10);
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p rflux-place`
Expected: 测试通过

- [ ] **Step 5: Commit**

```bash
git add crates/place/
git commit -m "feat(place): add partition placer for large-scale circuits"
```

### Task B3: DSE 并行化

**Covers:** [S3]

**Files:**
- Modify: `crates/flow/src/lib.rs`

- [ ] **Step 1: 在 DseConfig 中添加 combinations 方法**

```rust
impl DseConfig {
    pub fn combinations(&self) -> impl Iterator<Item = (f64, f64, f64, f64, usize)> + '_ {
        self.clock_period_ps_values.iter().flat_map(move |&cp| {
            self.prefer_ptl_from_length_um_values.iter().flat_map(move |&ptl| {
                self.detour_margin_um_values.iter().flat_map(move |&dm| {
                    self.min_hold_jtl_length_um_values.iter().flat_map(move |&jtl| {
                        self.sfq_phase_count_values.iter().map(move |&pc| (cp, ptl, dm, jtl, pc))
                    })
                })
            })
        })
    }
}
```

- [ ] **Step 2: 实现 run_dse_parallel**

在 `FlowRunner` impl 中添加：

```rust
pub fn run_dse_parallel(
    &mut self,
    netlist: &Netlist,
    pdk: &Pdk,
    base_config: &FlowConfig,
    dse_config: &DseConfig,
) -> DseReport {
    use rayon::prelude::*;

    let combos: Vec<_> = dse_config.combinations().collect();

    let results: Vec<_> = combos
        .par_iter()
        .filter_map(|&(clock_period_ps, prefer_ptl, detour_margin, min_hold_jtl, phase_count)| {
            let config = flow_config_from_dse_params(
                base_config,
                clock_period_ps,
                prefer_ptl,
                detour_margin,
                min_hold_jtl,
                phase_count,
            );
            let mut trial_netlist = netlist.clone();
            match self.compile_layout(&mut trial_netlist, pdk, &config) {
                Ok(report) => {
                    let (coupling_score, high_coupling_nets) =
                        match self.compile_artifacts(&mut netlist.clone(), pdk, &config) {
                            Ok(artifacts) => {
                                let cm = rflux_route::CouplingMap::build(
                                    &artifacts.routing.routes,
                                    10.0,
                                );
                                (cm.total_coupling_score(), cm.high_coupling_nets(0.1))
                            }
                            Err(_) => (0.0, 0),
                        };
                    Some(dse_point_from_report(
                        &report,
                        clock_period_ps,
                        prefer_ptl,
                        detour_margin,
                        min_hold_jtl,
                        phase_count,
                        coupling_score,
                        high_coupling_nets,
                    ))
                }
                Err(_) => None,
            }
        })
        .collect();

    let mut points = results;
    let total_evaluated = combos.len();
    let total_failed = total_evaluated - points.len();

    let pareto_indices = compute_pareto_front(&points);
    for idx in &pareto_indices {
        points[*idx].is_pareto_optimal = true;
    }
    let pareto_front: Vec<DsePoint> = pareto_indices.iter().map(|&i| points[i].clone()).collect();
    let recommended_idx = select_recommended(&points);
    let recommended = recommended_idx.map(|i| points[i].clone());

    DseReport {
        points,
        pareto_front,
        total_evaluated,
        total_failed,
        recommended,
    }
}
```

注意：`run_dse_parallel` 中的 `self.compile_layout` 调用需要 `&mut self`，但 `par_iter` 闭包不能捕获 `&mut`。需要将 `FlowRunner` 的状态提取为可共享结构，或者改用 `std::thread::scope` + 分区的方式。实际实现时需要调整。

**实际方案**：使用 `std::thread::scope` 替代 rayon 的 par_iter，每个线程创建独立的 FlowRunner：

```rust
pub fn run_dse_parallel(
    &self,
    netlist: &Netlist,
    pdk: &Pdk,
    base_config: &FlowConfig,
    dse_config: &DseConfig,
) -> DseReport {
    let combos: Vec<_> = dse_config.combinations().collect();

    let results: Vec<Option<DsePoint>> = std::thread::scope(|s| {
        let handles: Vec<_> = combos
            .iter()
            .map(|&(cp, ptl, dm, jtl, pc)| {
                s.spawn(move || {
                    let config = flow_config_from_dse_params(base_config, cp, ptl, dm, jtl, pc);
                    let mut flow = FlowRunner::new();
                    let mut trial_netlist = netlist.clone();
                    match flow.compile_layout(&mut trial_netlist, pdk, &config) {
                        Ok(report) => Some(dse_point_from_report(&report, cp, ptl, dm, jtl, pc, 0.0, 0)),
                        Err(_) => None,
                    }
                })
            })
            .collect();

        handles
            .into_iter()
            .map(|h| h.join().unwrap_or(None))
            .collect()
    });

    let mut points: Vec<DsePoint> = results.into_iter().flatten().collect();
    // ... pareto front calculation same as run_dse
    let pareto_indices = compute_pareto_front(&points);
    for idx in &pareto_indices {
        points[*idx].is_pareto_optimal = true;
    }
    let pareto_front: Vec<DsePoint> = pareto_indices.iter().map(|&i| points[i].clone()).collect();
    let recommended_idx = select_recommended(&points);
    let recommended = recommended_idx.map(|i| points[i].clone());

    DseReport {
        points,
        pareto_front,
        total_evaluated: combos.len(),
        total_failed: combos.len() - points.len(),
        recommended,
    }
}
```

- [ ] **Step 3: 在 CLI 的 DSE 命令中添加 --parallel 标志**

在 `DseArgs` 中添加：

```rust
#[arg(long)]
parallel: bool,
```

命令处理中：

```rust
let report = if args.parallel {
    flow.run_dse_parallel(&netlist, &pdk, &config, &dse_config)
} else {
    flow.run_dse(&netlist, &pdk, &config, &dse_config)
};
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p rflux-flow`
Expected: 所有测试通过

- [ ] **Step 5: Commit**

```bash
git add crates/flow/ crates/place/
git commit -m "feat(flow,place): add parallel DSE and partition placement for large circuits"
```

---

## 最终验证

### Task F1: 全量测试

- [ ] **Step 1: 运行全部测试**

Run: `cargo test --workspace`
Expected: 所有测试通过

- [ ] **Step 2: 运行 clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: 无警告

- [ ] **Step 3: 验证 CLI 命令**

```bash
cargo run -- pdk-minimal --format yaml --output test.yaml
cargo run -- pdk-validate --input test.yaml
cargo run -- analyze-margin --input test.bench --method monte-carlo --samples 100 --param "jtl_impedance_ohm,1.8,2.2,uniform"
```
