use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use rflux_ir::Netlist;
use rflux_route::RoutingReport;
use rflux_tech::Pdk;
use rflux_timing::{StaticTimingAnalyzer, TimingConfig};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MarginError {
    #[error("invalid margin configuration: {0}")]
    InvalidConfig(String),
    #[error("margin analysis failed: {0}")]
    AnalysisFailed(String),
}

impl MarginError {
    pub fn code(&self) -> &'static str {
        match self {
            MarginError::InvalidConfig(_) => "RFLOW-MARGIN-001",
            MarginError::AnalysisFailed(_) => "RFLOW-MARGIN-002",
        }
    }
}

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

fn sample_parameter(rng: &mut StdRng, param: &MarginParameter) -> f64 {
    match param.distribution {
        Distribution::Uniform => rng.gen_range(param.min..=param.max),
        Distribution::Normal { sigma_ratio } => {
            let sigma = (param.max - param.min) * sigma_ratio / 2.0;
            let mean = param.nominal;
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

    fn cartesian(acc: Vec<Vec<f64>>, remaining: &[Vec<f64>]) -> Vec<Vec<f64>> {
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
        cartesian(result, &remaining[1..])
    }

    let combos = cartesian(vec![vec![]], &param_values);
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct YieldOptimizerConfig {
    pub max_iterations: usize,
    pub samples_per_iteration: usize,
    pub improvement_threshold: f64,
    pub seed: u64,
}

impl Default for YieldOptimizerConfig {
    fn default() -> Self {
        Self {
            max_iterations: 50,
            samples_per_iteration: 100,
            improvement_threshold: 0.01,
            seed: 42,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct YieldReport {
    pub initial_yield: f64,
    pub optimized_yield: f64,
    pub iterations: usize,
    pub optimal_parameters: Vec<(String, f64)>,
    pub improvement: f64,
}

pub struct YieldOptimizer {
    config: YieldOptimizerConfig,
}

impl YieldOptimizer {
    pub fn new(config: YieldOptimizerConfig) -> Self {
        Self { config }
    }

    pub fn optimize(
        &self,
        netlist: &Netlist,
        routing: &RoutingReport,
        pdk: &Pdk,
        margin_config: &MarginConfig,
    ) -> YieldReport {
        let mut rng = StdRng::seed_from_u64(self.config.seed);

        let initial_report = analyze_margin(netlist, routing, pdk, margin_config);
        let mut best_yield = initial_report.yield_estimate;
        let mut best_params: Vec<(String, f64)> = margin_config
            .parameters
            .iter()
            .map(|p| (p.name.clone(), p.nominal))
            .collect();

        for _iter in 0..self.config.max_iterations {
            let candidate_params: Vec<MarginParameter> = margin_config
                .parameters
                .iter()
                .map(|p| {
                    let delta = (p.max - p.min) * 0.1;
                    let new_nominal = p.nominal + rng.gen_range(-delta..delta);
                    let clamped = new_nominal.clamp(p.min, p.max);
                    MarginParameter {
                        name: p.name.clone(),
                        nominal: clamped,
                        min: p.min,
                        max: p.max,
                        distribution: p.distribution,
                    }
                })
                .collect();

            let candidate_config = MarginConfig {
                parameters: candidate_params,
                method: MarginMethod::MonteCarlo {
                    samples: self.config.samples_per_iteration,
                },
                seed: rng.gen(),
                clock_period_ps: margin_config.clock_period_ps,
            };

            let candidate_report = analyze_margin(netlist, routing, pdk, &candidate_config);

            if candidate_report.yield_estimate > best_yield + self.config.improvement_threshold {
                best_yield = candidate_report.yield_estimate;
                best_params = candidate_config
                    .parameters
                    .iter()
                    .map(|p| (p.name.clone(), p.nominal))
                    .collect();
            }
        }

        YieldReport {
            initial_yield: initial_report.yield_estimate,
            optimized_yield: best_yield,
            iterations: self.config.max_iterations,
            optimal_parameters: best_params,
            improvement: best_yield - initial_report.yield_estimate,
        }
    }
}

fn apply_parameters_to_pdk(base: &Pdk, params: &[(String, f64)]) -> Pdk {
    let mut pdk = base.clone();
    for (name, value) in params {
        match name.as_str() {
            "jtl_impedance_ohm" => pdk.jtl_impedance_ohm = *value,
            "ptl_impedance_ohm" => pdk.ptl_impedance_ohm = *value,
            "jtl_propagation_delay_ps_per_um" => pdk.jtl_propagation_delay_ps_per_um = *value,
            "ptl_propagation_delay_ps_per_um" => pdk.ptl_propagation_delay_ps_per_um = *value,
            other => {
                if let Some(rest) = other.strip_prefix("cell_timing.") {
                    if let Some((kind_str, field)) = rest.rsplit_once('.') {
                        for timing in pdk.cell_timing.iter_mut() {
                            if format!("{:?}", timing.kind) == kind_str {
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
    }
    pdk
}

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
        let report = analyzer.analyze(netlist, routing, &modified_pdk, &timing_config, None);
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

    let param_names: Vec<String> = config.parameters.iter().map(|p| p.name.clone()).collect();
    let sensitivity = compute_sensitivity(&samples, &param_names);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_sampling_within_bounds() {
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
    fn normal_sampling_clamped() {
        let param = MarginParameter {
            name: "ic".to_string(),
            nominal: 1.0,
            min: 0.8,
            max: 1.2,
            distribution: Distribution::Normal { sigma_ratio: 0.1 },
        };
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..1000 {
            let val = sample_parameter(&mut rng, &param);
            assert!(val >= 0.8 && val <= 1.2);
        }
    }

    #[test]
    fn mc_sample_count() {
        let config = MarginConfig {
            parameters: vec![MarginParameter {
                name: "a".to_string(),
                nominal: 1.0,
                min: 0.0,
                max: 2.0,
                distribution: Distribution::Uniform,
            }],
            method: MarginMethod::MonteCarlo { samples: 100 },
            seed: 42,
            clock_period_ps: 120.0,
        };
        let mut rng = StdRng::seed_from_u64(42);
        let samples = generate_mc_samples(&config, &mut rng);
        assert_eq!(samples.len(), 100);
        assert_eq!(samples[0].len(), 1);
    }

    #[test]
    fn boundary_combinations_count() {
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
        assert_eq!(samples.len(), 16);
    }

    #[test]
    fn boundary_empty_params() {
        let config = MarginConfig {
            parameters: vec![],
            method: MarginMethod::BoundarySweep { steps_per_param: 3 },
            seed: 42,
            clock_period_ps: 120.0,
        };
        let samples = generate_boundary_samples(&config);
        assert!(samples.is_empty());
    }

    #[test]
    fn analyze_margin_empty_netlist() {
        let netlist = Netlist::new();
        let routing = RoutingReport {
            routes: Vec::new(),
            total_length_um: 0.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 0,
            ptl_routes: 0,
        clock_routes: 0,
        data_routes: 0,
        peak_channel_usage: 0,
        co_routed: false,
        };
        let pdk = Pdk::minimal("test");
        let config = MarginConfig {
            parameters: vec![MarginParameter {
                name: "jtl_impedance_ohm".to_string(),
                nominal: 2.0,
                min: 1.8,
                max: 2.2,
                distribution: Distribution::Uniform,
            }],
            method: MarginMethod::MonteCarlo { samples: 10 },
            seed: 42,
            clock_period_ps: 120.0,
        };
        let report = analyze_margin(&netlist, &routing, &pdk, &config);
        assert_eq!(report.total_samples, 10);
        assert!(report.yield_estimate >= 0.0 && report.yield_estimate <= 1.0);
        assert_eq!(report.sensitivity.len(), 1);
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(
            MarginError::InvalidConfig("".to_string()).code(),
            "RFLOW-MARGIN-001"
        );
        assert_eq!(
            MarginError::AnalysisFailed("".to_string()).code(),
            "RFLOW-MARGIN-002"
        );
    }

    #[test]
    fn yield_optimizer_improves_or_maintains() {
        let netlist = Netlist::new();
        let routing = RoutingReport {
            routes: Vec::new(),
            total_length_um: 0.0,
            total_detour_overhead_um: 0.0,
            detoured_routes: 0,
            jtl_routes: 0,
            ptl_routes: 0,
        clock_routes: 0,
        data_routes: 0,
        peak_channel_usage: 0,
        co_routed: false,
        };
        let pdk = Pdk::minimal("test");
        let margin_config = MarginConfig {
            parameters: vec![MarginParameter {
                name: "jtl_impedance_ohm".to_string(),
                nominal: 2.0,
                min: 1.8,
                max: 2.2,
                distribution: Distribution::Uniform,
            }],
            method: MarginMethod::MonteCarlo { samples: 50 },
            seed: 42,
            clock_period_ps: 120.0,
        };
        let optimizer = YieldOptimizer::new(YieldOptimizerConfig::default());
        let report = optimizer.optimize(&netlist, &routing, &pdk, &margin_config);
        assert!(report.optimized_yield >= report.initial_yield - 0.01);
        assert!(!report.optimal_parameters.is_empty());
    }
}
