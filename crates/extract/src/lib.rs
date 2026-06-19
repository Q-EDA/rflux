use rflux_ir::PinRef;
use rflux_route::{NetRoute, RouteMode, RoutingReport};
use rflux_tech::Pdk;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("invalid extraction parameters: {0}")]
    InvalidParams(String),
}

impl ExtractError {
    pub fn code(&self) -> &'static str {
        match self {
            ExtractError::InvalidParams(_) => "RFLOW-EXTRACT-001",
        }
    }
}

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
        let kr = self.config.kinetic_inductance_ratio;

        let ratio = 1.0 + 2.0 * h / w.max(0.001);
        let l_geo_per_um = if ratio >= 1.0 {
            0.5 * ratio.acosh()
        } else {
            0.5
        };

        let l_per_um = l_geo_per_um * (1.0 + kr);

        let c_per_um = 0.0885 * eps_r * (w + 2.0 * t) / h.max(0.001);

        let l_h = l_per_um * 1e-12;
        let c_f = c_per_um * 1e-15;
        let z0 = if c_f > 0.0 { (l_h / c_f).sqrt() } else { 0.0 };

        let delay_per_um = if c_f > 0.0 && l_h > 0.0 {
            (l_h * c_f).sqrt() * 1e12
        } else {
            0.0
        };

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

    fn extract_jtl(&self, length_um: f64) -> ExtractedParasitics {
        let delay_per_um = 0.15;
        let z0 = 2.0;
        let l_per_um = 2.0;
        let c_per_um = l_per_um / (z0 * z0);

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

#[cfg(test)]
mod tests {
    use super::*;
    use rflux_ir::NodeId;

    fn make_route(mode: RouteMode, length_um: f64) -> NetRoute {
        NetRoute {
            from: PinRef { node: NodeId(0), port: 0 },
            to: PinRef { node: NodeId(1), port: 0 },
            mode,
            segments: vec![],
            direct_length_um: length_um,
            length_um,
        }
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(
            ExtractError::InvalidParams("".to_string()).code(),
            "RFLOW-EXTRACT-001"
        );
    }

    #[test]
    fn ptl_parasitics_positive_values() {
        let config = ParasiticConfig::default();
        let extractor = ParasiticExtractor::new(config);
        let route = make_route(RouteMode::Ptl, 100.0);
        let net_p = extractor.extract_net(&route);
        let p = &net_p.parasitics;
        assert!(p.c_per_um > 0.0);
        assert!(p.l_per_um > 0.0);
        assert!(p.z0_ohm > 0.0);
        assert!(p.delay_ps_per_um > 0.0);
        assert_eq!(p.total_length_um, 100.0);
    }

    #[test]
    fn jtl_parasitics_consistent_with_pdk() {
        let config = ParasiticConfig::default();
        let extractor = ParasiticExtractor::new(config);
        let route = make_route(RouteMode::Jtl, 50.0);
        let p = extractor.extract_net(&route);
        assert!((p.parasitics.delay_ps_per_um - 0.15).abs() < 1e-6);
        assert!((p.parasitics.z0_ohm - 2.0).abs() < 1e-6);
    }

    #[test]
    fn extract_report_aggregates() {
        let config = ParasiticConfig::default();
        let extractor = ParasiticExtractor::new(config);
        let routing = RoutingReport {
            routes: vec![
                make_route(RouteMode::Ptl, 100.0),
                make_route(RouteMode::Jtl, 50.0),
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

    #[test]
    fn kinetic_inductance_increases_delay() {
        let mut config_low = ParasiticConfig::default();
        config_low.kinetic_inductance_ratio = 0.0;
        let mut config_high = ParasiticConfig::default();
        config_high.kinetic_inductance_ratio = 2.0;
        let ext_low = ParasiticExtractor::new(config_low);
        let ext_high = ParasiticExtractor::new(config_high);
        let route = make_route(RouteMode::Ptl, 100.0);
        let p_low = ext_low.extract_net(&route);
        let p_high = ext_high.extract_net(&route);
        assert!(p_high.parasitics.total_delay_ps > p_low.parasitics.total_delay_ps);
    }
}
