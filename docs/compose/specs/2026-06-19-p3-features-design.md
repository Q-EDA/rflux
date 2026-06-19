# P3 功能实现设计

日期：2026-06-19

## [S1] 概述

实现三项 P3 功能：SPICE 前端、技术映射扩展、错误码统一。

## [S2] SPICE 前端

复用 rflux-sim 的 deck parser，添加网表提取路径。

### 支持的 SPICE 元素
- `.subckt` / `.ends` 层次结构
- `.include` / `.lib` 文件引用
- `X` 实例（子电路实例化）
- `J` 实例（JJ 器件）
- `R`/`L`/`C` 无源器件
- `T` 传输线
- `K` 互感
- `V`/`I` 源（可选）

### 新增结构

```rust
pub struct SpiceDevice {
    pub name: String,
    pub kind: SpiceDeviceKind,
    pub connections: Vec<String>,
    pub params: BTreeMap<String, f64>,
}

pub enum SpiceDeviceKind {
    SubcktInstance(String),  // subcircuit name
    Jj,
    Resistor,
    Inductor,
    Capacitor,
    TransmissionLine,
    MutualInductance,
    VoltageSource,
    CurrentSource,
}

pub fn parse_spice_netlist(deck: &str) -> Result<Vec<SpiceDevice>, SimulationError>
pub fn spice_to_ir(devices: &[SpiceDevice]) -> Result<Netlist, String>
```

## [S3] 技术映射扩展

增强 rflux-synth 的 TechMapper：

1. **复杂门推断**：从 AND/OR/NOT 组合推断 AOI/OAI 模式
2. **面积感知**：当多个 cell 匹配时选择面积最小的
3. **映射报告**：输出未映射节点、映射覆盖率

## [S4] 错误码统一

为所有 crate 的错误类型添加 `code()` 方法：

- `rflux-drc`：RFLOW-DRC-001 (trace_spacing), RFLOW-DRC-002 (jj_spacing), ...
- `rflux-margin`：RFLOW-MARGIN-001 (invalid_config), ...
- `rflux-extract`：RFLOW-EXTRACT-001 (invalid_params), ...
- `rflux-verilog`：RFLOW-VERILOG-001 (parse_error), RFLOW-VERILOG-002 (elab_error), ...

## [S5] 测试策略

- SPICE：解析 .cir 文件 → 提取设备 → 转换 IR → 验证节点数
- 技术映射：AOI 门应被识别为复杂单元
- 错误码：每个 crate 的 error_code 稳定性测试
