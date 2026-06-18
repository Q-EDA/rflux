# SFQ EDA 后端增强设计：裕度分析 + 大规模电路 + PDK YAML

日期：2026-06-18

---

## [S1] 概述

rflux 作为独立的 SFQ EDA 后端工具，需要补充三项能力：

1. **参数化裕度分析**：评估工艺参数变化下的电路鲁棒性
2. **大规模电路支持**：万级节点以上电路的分区布局与并行处理
3. **PDK YAML 格式**：兼容 qeda-pro 等生态的 YAML PDK 文件

三项功能相互独立，可并行开发。

---

## [S2] 参数化裕度分析

### 目标

在 PDK 参数（JJ 临界电流 Ic、结电阻 Rn、PTL 阻抗、温度等）变化范围内，评估电路的时序裕度和良率。

### 方法

#### 蒙特卡洛分析

- 随机采样 N 个参数组合（默认 N=1000）
- 对每个样本修改 PDK 参数，运行 STA
- 统计：yield（slack>=0 的比例）、slack 分布、最差工况

#### 边界扫描

- 在每个参数的 min/max 范围内，按固定步长系统扫描
- 对所有参数组合运行 STA
- 输出：最差工况组合、参数敏感度（slack 对参数的偏导数近似）

### 新增 crate：`crates/margin/`

依赖：`rflux-timing`、`rflux-tech`、`rflux-ir`、`rflux-route`

### 核心结构

```rust
pub enum MarginMethod {
    MonteCarlo { samples: usize },
    BoundarySweep { steps_per_param: usize },
}

pub enum Distribution {
    Uniform,
    Normal { sigma_ratio: f64 },
}

pub enum MarginMetric {
    WorstSlack,
    Yield,
    CriticalPathDelay,
}

pub struct MarginParameter {
    pub name: String,
    pub nominal: f64,
    pub min: f64,
    pub max: f64,
    pub distribution: Distribution,
}

pub struct MarginConfig {
    pub parameters: Vec<MarginParameter>,
    pub method: MarginMethod,
    pub metric: MarginMetric,
    pub seed: u64,
    pub clock_period_ps: f64,
}

pub struct MarginSample {
    pub parameter_values: Vec<(String, f64)>,
    pub worst_setup_slack_ps: f64,
    pub worst_hold_slack_ps: f64,
    pub critical_path_delay_ps: f64,
    pub setup_violations: usize,
    pub hold_violations: usize,
}

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

### 工作流

1. 用户指定要扫描的参数列表和范围
2. 引擎根据 method 生成参数样本
3. 对每个样本：修改 PDK → 运行 STA → 记录结果
4. 汇总统计：yield、敏感度、最差工况

### CLI

```
rflux analyze-margin --input circuit.json --pdk pdk.json \
  --method monte-carlo --samples 1000 \
  --param "cell_timing.GenericGate.intrinsic_delay_ps,6.0,10.0,normal" \
  --param "jtl_impedance_ohm,1.8,2.2,uniform"
```

### Python API

```python
report = rflux.analyze_margin(
    netlist, pdk, routing,
    method="monte_carlo", samples=1000,
    parameters=[
        {"name": "ic_scale", "min": 0.9, "max": 1.1, "distribution": "uniform"},
    ],
)
```

---

## [S3] 大规模电路支持

### 目标

提升万级节点以上电路的处理能力，通过分区和并行化。

### 改动范围

- `crates/place/`：新增分区布局器
- `crates/flow/`：DSE 并行化
- `Cargo.toml`：新增 `rayon` 依赖

### 分区布局器

```rust
pub struct PartitionConfig {
    pub max_partition_size: usize,   // 每个分区最大节点数，默认 500
    pub overlap_margin_um: f64,      // 分区间重叠边距，默认 20.0
    pub enable_partitioning: bool,   // 是否启用分区，默认 false
}
```

**分区策略**：基于拓扑层级的图分割

1. 对 netlist 做拓扑排序，得到层级
2. 当层级宽度超过 max_partition_size 时，将该层级切分为多个分区
3. 分区间保留 overlap_margin 用于跨分区布线
4. 每个分区内独立布局，最后合并坐标

### DSE 并行化

当前 `run_dse` 是串行的 5 层嵌套循环。改为：

```rust
use rayon::prelude::*;

pub fn run_dse_parallel(
    &mut self,
    netlist: &Netlist,
    pdk: &Pdk,
    base_config: &FlowConfig,
    dse_config: &DseConfig,
) -> DseReport {
    // 生成所有参数组合
    let combos: Vec<_> = dse_config.combinations().collect();
    // 并行评估
    let results: Vec<_> = combos.par_iter()
        .filter_map(|combo| self.evaluate_combo(netlist, pdk, base_config, combo))
        .collect();
    // 合并结果，计算 Pareto 前沿
    ...
}
```

### 蒙特卡洛并行化

margin crate 的蒙特卡洛分析天然可并行：

```rust
let samples: Vec<_> = (0..config.samples)
    .into_par_iter()
    .map(|i| self.evaluate_sample(i, &param_sets[i]))
    .collect();
```

---

## [S4] PDK YAML 格式支持

### 目标

支持 YAML 格式的 PDK 文件读写，兼容 qeda-pro 的 YAML PDK 生态。

### 改动范围

- `Cargo.toml`：workspace 新增 `serde_yaml` 依赖
- `crates/tech/Cargo.toml`：添加 `serde_yaml` 依赖
- `crates/tech/src/lib.rs`：添加 `from_yaml()` / `to_yaml()` 方法
- `crates/cli/src/main.rs`：自动检测格式、新增 `--format` 参数
- `crates/io/src/lib.rs`：通用 YAML/JSON 自动检测读取

### 核心改动

```rust
// crates/tech/src/lib.rs
impl Pdk {
    pub fn from_yaml(serialized: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(serialized)
    }
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }
    /// 自动检测格式（根据内容或扩展名）
    pub fn from_auto(content: &str, path: Option<&Path>) -> Result<Self, String> {
        if let Some(path) = path {
            match path.extension().and_then(|e| e.to_str()) {
                Some("yaml" | "yml") => return Self::from_yaml(content).map_err(|e| e.to_string()),
                Some("json") => return Self::from_json(content).map_err(|e| e.to_string()),
                _ => {}
            }
        }
        // 尝试 JSON，失败则尝试 YAML
        Self::from_json(content)
            .or_else(|_| Self::from_yaml(content).map_err(|e| e.to_string()))
    }
}
```

### CLI 改动

```
rflux pdk-minimal --format yaml          # 输出 YAML 格式
rflux pdk-minimal --format json          # 输出 JSON 格式（默认）
rflux compile-layout --pdk pdk.yaml      # 自动检测 YAML
rflux pdk-validate --pdk pdk.yml         # 自动检测 YAML
```

所有接受 `--pdk` 参数的命令都自动检测格式。

### Python API

```python
pdk = rflux.Pdk.from_yaml(yaml_string)
yaml_str = pdk.to_yaml()
```

---

## [S5] 依赖变更

```toml
# Cargo.toml [workspace.dependencies]
serde_yaml = "0.9"
rayon = "1.10"

# crates/margin/Cargo.toml
[dependencies]
rflux-ir = { path = "../ir" }
rflux-tech = { path = "../tech" }
rflux-timing = { path = "../timing" }
rflux-route = { path = "../route" }
serde = { workspace = true }
rand = "0.8"

# crates/tech/Cargo.toml
serde_yaml = { workspace = true, optional = true }
[features]
yaml = ["serde_yaml"]
```

---

## [S6] 测试策略

### 裕度分析

- 单元测试：参数采样正确性、敏感度计算、yield 统计
- 集成测试：用 minimal PDK + 小电路跑 MC 100 样本，验证 yield 在合理范围

### 大规模电路

- 性能基准：1000/5000/10000 节点电路的布局布线时间
- 正确性：分区布局结果的时序不应比非分区差（允许微小差异）

### PDK YAML

- 单元测试：JSON PDK ↔ YAML PDK 双向转换一致性
- 集成测试：用 YAML PDK 跑完整 flow

---

## [S7] 实现顺序

建议并行开发：

1. **PDK YAML**（最简单，1-2 天）：tech + cli + io
2. **裕度分析**（中等复杂度，3-5 天）：新增 margin crate + cli + py 绑定
3. **大规模电路**（最复杂，5-7 天）：place 分区 + flow 并行 + 性能测试
