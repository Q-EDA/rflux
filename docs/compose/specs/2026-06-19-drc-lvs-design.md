# DRC/LVS 设计

日期：2026-06-19

## [S1] 概述

为 rflux 添加物理验证能力：DRC（Design Rule Check）检查 layout 是否满足 SFQ 物理设计规则，LVS（Layout Versus Schematic）对比 layout 提取的网表与原始原理图。

## [S2] DRC 规则集

```rust
pub struct DrcRuleSet {
    pub min_trace_width_um: f64,
    pub min_trace_spacing_um: f64,
    pub min_jj_spacing_um: f64,
    pub max_ptl_length_um: Option<f64>,
    pub cell_boundary_margin_um: f64,
}
```

### 检查项

1. **trace_spacing**：不同 net 的 route segment 之间最小 Manhattan 距离 < min_trace_spacing_um
2. **ptl_length**：PTL route length 在 PDK 禁止范围内
3. **jj_spacing**：两个 cell 实例之间 Manhattan 距离 < min_jj_spacing_um
4. **cell_boundary**：cell 位置超出 layout 边界（含 margin）

## [S3] DRC 输出

```rust
pub struct DrcViolation {
    pub rule: String,
    pub severity: DrcSeverity,
    pub location: Option<Point>,
    pub detail: String,
}

pub struct DrcReport {
    pub violations: Vec<DrcViolation>,
    pub checked_rules: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub passed: bool,
}
```

## [S4] LVS 流程

1. 从 Placement 获取 cell 实例列表
2. 从 RoutingReport 获取连接关系
3. 构建 layout 提取网表（cell 类型 + 连接）
4. 与原始 Netlist 对比：
   - 设备数量和类型是否匹配
   - 每条 net 的连接关系是否匹配
5. 输出差异报告

```rust
pub struct LvsReport {
    pub matched: bool,
    pub device_count_mismatch: bool,
    pub connectivity_mismatch: bool,
    pub missing_devices: Vec<String>,
    pub extra_devices: Vec<String>,
    pub net_mismatches: Vec<NetMismatch>,
    pub checked_nets: usize,
    pub matched_nets: usize,
}
```

## [S5] PDK 扩展

在 Pdk 中新增可选 DRC 规则：

```rust
pub struct SfqDrcRules {
    pub min_trace_width_um: f64,
    pub min_trace_spacing_um: f64,
    pub min_jj_spacing_um: f64,
    pub cell_boundary_margin_um: f64,
}
```

## [S6] 集成点

1. `crates/drc/`：新增 crate
2. `crates/flow/`：verify_layout 中集成 DRC/LVS
3. `crates/cli/`：`check-drc` 子命令
4. Python：`check_drc()` / `check_lvs()` 绑定

## [S7] 测试策略

- DRC：无违规的干净 layout 应 pass
- DRC：故意插入过近的 cell 应检出 jj_spacing 违规
- DRC：PTL 超长应检出 ptl_length 违规
- LVS：layout 与 schematic 一致应 matched
- LVS：删除一个 route 应检出 connectivity_mismatch
