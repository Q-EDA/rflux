use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InterconnectKind {
    Jtl,
    Ptl,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimingPoint {
    pub length_um: f64,
    pub delay_ps: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CellTimingModel {
    pub kind: SfCellKind,
    pub intrinsic_delay_ps: f64,
    pub setup_ps: f64,
    pub hold_ps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedCellTimingModel {
    pub cell_name: String,
    pub timing: CellTimingModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterconnectTimingModel {
    pub kind: InterconnectKind,
    pub points: Vec<TimingPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SfCellKind {
    GenericGate,
    Macro,
    Splitter,
    Dff,
    Jtl,
    Ptl,
    Port,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SfCell {
    pub name: String,
    pub kind: SfCellKind,
    pub area_um2: f64,
    pub pipeline_stages: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SfCellLibrary {
    pub name: String,
    pub cells: Vec<SfCell>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterizationDelayDetail {
    pub name: String,
    pub delay_ps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterizationArcDelay {
    pub name: String,
    pub driver_cell_name: String,
    pub from_port: u16,
    pub sink_cell_name: String,
    pub to_port: u16,
    pub delay_ps: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CharacterizationArtifactMetadata {
    pub waveform_path: Option<String>,
    pub simulated_delay_ps: Option<f64>,
    pub sta_derived_delay_ps: Option<f64>,
    pub delay_calibration_sigma_ps: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delay_details: Vec<CharacterizationDelayDetail>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arc_delays: Vec<CharacterizationArcDelay>,
}

impl CharacterizationArtifactMetadata {
    pub fn delay_detail_spread_sigma_ps(&self) -> f64 {
        if self.delay_details.len() < 2 {
            return 0.0;
        }
        let max_delay = self
            .delay_details
            .iter()
            .map(|detail| detail.delay_ps)
            .fold(0.0_f64, f64::max);
        let min_delay = self
            .delay_details
            .iter()
            .map(|detail| detail.delay_ps)
            .fold(f64::INFINITY, f64::min);
        ((max_delay - min_delay) * 0.5).max(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedCharacterizationMetadata {
    pub cell_name: String,
    pub metadata: CharacterizationArtifactMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterizedCellLibraryEntry {
    pub cell: SfCell,
    pub timing: CellTimingModel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<CharacterizationArtifactMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterizedCellLibraryBundle {
    pub entries: Vec<CharacterizedCellLibraryEntry>,
}

impl SfCellLibrary {
    pub fn minimal() -> Self {
        Self {
            name: "minimal-sfq".to_string(),
            cells: vec![
                SfCell {
                    name: "sfq_gate".to_string(),
                    kind: SfCellKind::GenericGate,
                    area_um2: 12.0,
                    pipeline_stages: 1,
                },
                SfCell {
                    name: "sfq_macro".to_string(),
                    kind: SfCellKind::Macro,
                    area_um2: 48.0,
                    pipeline_stages: 2,
                },
                SfCell {
                    name: "sfq_splitter".to_string(),
                    kind: SfCellKind::Splitter,
                    area_um2: 10.0,
                    pipeline_stages: 0,
                },
                SfCell {
                    name: "sfq_dff".to_string(),
                    kind: SfCellKind::Dff,
                    area_um2: 18.0,
                    pipeline_stages: 1,
                },
                SfCell {
                    name: "sfq_jtl".to_string(),
                    kind: SfCellKind::Jtl,
                    area_um2: 6.0,
                    pipeline_stages: 0,
                },
                SfCell {
                    name: "sfq_ptl".to_string(),
                    kind: SfCellKind::Ptl,
                    area_um2: 4.0,
                    pipeline_stages: 0,
                },
                SfCell {
                    name: "sfq_port".to_string(),
                    kind: SfCellKind::Port,
                    area_um2: 0.0,
                    pipeline_stages: 0,
                },
            ],
        }
    }

    pub fn find_by_kind(&self, kind: SfCellKind) -> Option<&SfCell> {
        self.cells.iter().find(|cell| cell.kind == kind)
    }

    pub fn find_by_name(&self, name: &str) -> Option<&SfCell> {
        self.cells.iter().find(|cell| cell.name == name)
    }

    pub fn upsert(&mut self, cell: SfCell) {
        if let Some(existing) = self.cells.iter_mut().find(|existing| existing.name == cell.name) {
            *existing = cell;
        } else {
            self.cells.push(cell);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LengthRange {
    pub min_um: f64,
    pub max_um: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pdk {
    pub name: String,
    pub metal_layers: u8,
    pub ptl_forbidden_ranges: Vec<LengthRange>,
    pub cell_library: SfCellLibrary,
    pub cell_timing: Vec<CellTimingModel>,
    pub named_cell_timing: Vec<NamedCellTimingModel>,
    pub characterized_cell_metadata: Vec<NamedCharacterizationMetadata>,
    pub interconnect_timing: Vec<InterconnectTimingModel>,
}

impl Pdk {
    pub fn minimal(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            metal_layers: 4,
            ptl_forbidden_ranges: Vec::new(),
            cell_library: SfCellLibrary::minimal(),
            cell_timing: vec![
                CellTimingModel {
                    kind: SfCellKind::GenericGate,
                    intrinsic_delay_ps: 8.0,
                    setup_ps: 6.0,
                    hold_ps: 4.0,
                },
                CellTimingModel {
                    kind: SfCellKind::Macro,
                    intrinsic_delay_ps: 14.0,
                    setup_ps: 8.0,
                    hold_ps: 5.0,
                },
                CellTimingModel {
                    kind: SfCellKind::Splitter,
                    intrinsic_delay_ps: 4.0,
                    setup_ps: 0.0,
                    hold_ps: 0.0,
                },
                CellTimingModel {
                    kind: SfCellKind::Dff,
                    intrinsic_delay_ps: 10.0,
                    setup_ps: 7.0,
                    hold_ps: 8.0,
                },
                CellTimingModel {
                    kind: SfCellKind::Jtl,
                    intrinsic_delay_ps: 3.0,
                    setup_ps: 0.0,
                    hold_ps: 0.0,
                },
                CellTimingModel {
                    kind: SfCellKind::Ptl,
                    intrinsic_delay_ps: 2.0,
                    setup_ps: 0.0,
                    hold_ps: 0.0,
                },
                CellTimingModel {
                    kind: SfCellKind::Port,
                    intrinsic_delay_ps: 0.0,
                    setup_ps: 0.0,
                    hold_ps: 0.0,
                },
            ],
            named_cell_timing: Vec::new(),
            characterized_cell_metadata: Vec::new(),
            interconnect_timing: vec![
                InterconnectTimingModel {
                    kind: InterconnectKind::Jtl,
                    points: vec![
                        TimingPoint {
                            length_um: 0.0,
                            delay_ps: 6.0,
                        },
                        TimingPoint {
                            length_um: 40.0,
                            delay_ps: 18.0,
                        },
                        TimingPoint {
                            length_um: 80.0,
                            delay_ps: 34.0,
                        },
                    ],
                },
                InterconnectTimingModel {
                    kind: InterconnectKind::Ptl,
                    points: vec![
                        TimingPoint {
                            length_um: 0.0,
                            delay_ps: 4.0,
                        },
                        TimingPoint {
                            length_um: 80.0,
                            delay_ps: 12.0,
                        },
                        TimingPoint {
                            length_um: 160.0,
                            delay_ps: 20.0,
                        },
                    ],
                },
            ],
        }
    }

    pub fn is_ptl_length_allowed(&self, length_um: f64) -> bool {
        !self
            .ptl_forbidden_ranges
            .iter()
            .any(|r| length_um >= r.min_um && length_um <= r.max_um)
    }

    pub fn cell_timing(&self, kind: SfCellKind) -> Option<&CellTimingModel> {
        self.cell_timing.iter().find(|model| model.kind == kind)
    }

    pub fn cell_timing_for_cell(&self, cell_name: &str, kind: SfCellKind) -> Option<&CellTimingModel> {
        self.named_cell_timing
            .iter()
            .find(|model| model.cell_name == cell_name)
            .map(|model| &model.timing)
            .or_else(|| self.cell_timing(kind))
    }

    pub fn cell_for_node(&self, cell_name: &str, kind: SfCellKind) -> Option<&SfCell> {
        self.cell_library
            .find_by_name(cell_name)
            .or_else(|| self.cell_library.find_by_kind(kind))
    }

    pub fn characterization_metadata_for_cell(
        &self,
        cell_name: &str,
    ) -> Option<&CharacterizationArtifactMetadata> {
        self.characterized_cell_metadata
            .iter()
            .find(|entry| entry.cell_name == cell_name)
            .map(|entry| &entry.metadata)
    }

    pub fn characterized_arc_delay_ps(
        &self,
        driver_cell_name: &str,
        from_port: u16,
        sink_cell_name: &str,
        to_port: u16,
    ) -> Option<f64> {
        let metadata = self.characterization_metadata_for_cell(driver_cell_name)?;
        metadata
            .arc_delays
            .iter()
            .find(|arc| {
                arc.driver_cell_name == driver_cell_name
                    && arc.from_port == from_port
                    && arc.sink_cell_name == sink_cell_name
                    && arc.to_port == to_port
            })
            .map(|arc| arc.delay_ps)
    }

    pub fn with_characterized_cell(&self, entry: CharacterizedCellLibraryEntry) -> Self {
        let mut updated = self.clone();
        let cell_name = entry.cell.name.clone();
        updated.cell_library.upsert(entry.cell);
        if let Some(existing) = updated
            .named_cell_timing
            .iter_mut()
            .find(|model| model.cell_name == cell_name)
        {
            existing.timing = entry.timing;
        } else {
            updated.named_cell_timing.push(NamedCellTimingModel {
                cell_name: cell_name.clone(),
                timing: entry.timing,
            });
        }
        if let Some(metadata) = entry.metadata {
            if let Some(existing) = updated
                .characterized_cell_metadata
                .iter_mut()
                .find(|entry| entry.cell_name == cell_name)
            {
                existing.metadata = metadata;
            } else {
                updated.characterized_cell_metadata.push(NamedCharacterizationMetadata {
                    cell_name,
                    metadata,
                });
            }
        }
        updated
    }

    pub fn with_characterized_library_entries(
        &self,
        entries: impl IntoIterator<Item = CharacterizedCellLibraryEntry>,
    ) -> Self {
        entries
            .into_iter()
            .fold(self.clone(), |pdk, entry| pdk.with_characterized_cell(entry))
    }

    pub fn with_characterized_library_bundle_json(
        &self,
        serialized_bundle: &str,
    ) -> Result<Self, serde_json::Error> {
        let bundle = serde_json::from_str::<CharacterizedCellLibraryBundle>(serialized_bundle)?;
        Ok(self.with_characterized_library_entries(bundle.entries))
    }

    pub fn with_characterized_library_json(
        &self,
        serialized_entry: &str,
    ) -> Result<Self, serde_json::Error> {
        if serialized_entry.contains("\"entries\"") {
            return self.with_characterized_library_bundle_json(serialized_entry);
        }
        let entry = serde_json::from_str::<CharacterizedCellLibraryEntry>(serialized_entry)?;
        Ok(self.with_characterized_cell(entry))
    }

    pub fn merge_characterized_library_json_strings(
        &self,
        serialized_entries: &[&str],
    ) -> Result<Self, serde_json::Error> {
        serialized_entries.iter().try_fold(self.clone(), |pdk, entry| {
            pdk.with_characterized_library_json(entry)
        })
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(serialized: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(serialized)
    }

    pub fn interconnect_delay_ps(&self, kind: InterconnectKind, length_um: f64) -> Option<f64> {
        let model = self.interconnect_timing.iter().find(|model| model.kind == kind)?;
        interpolate_delay(&model.points, length_um)
    }
}

fn interpolate_delay(points: &[TimingPoint], length_um: f64) -> Option<f64> {
    let first = points.first()?;
    if length_um <= first.length_um {
        return Some(first.delay_ps);
    }

    for window in points.windows(2) {
        let start = window[0];
        let end = window[1];
        if length_um <= end.length_um {
            let span = end.length_um - start.length_um;
            if span.abs() < f64::EPSILON {
                return Some(end.delay_ps);
            }
            let ratio = (length_um - start.length_um) / span;
            return Some(start.delay_ps + ratio * (end.delay_ps - start.delay_ps));
        }
    }

    let last = points.last()?;
    let prev = points.get(points.len().saturating_sub(2)).copied().unwrap_or(*last);
    let span = last.length_um - prev.length_um;
    if span.abs() < f64::EPSILON {
        return Some(last.delay_ps);
    }
    let slope = (last.delay_ps - prev.delay_ps) / span;
    Some(last.delay_ps + (length_um - last.length_um) * slope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdk_can_upsert_characterized_cell_entry() {
        let base = Pdk::minimal("test");
        let updated = base.with_characterized_cell(CharacterizedCellLibraryEntry {
            cell: SfCell {
                name: "compound_buf".to_string(),
                kind: SfCellKind::Macro,
                area_um2: 52.0,
                pipeline_stages: 2,
            },
            timing: CellTimingModel {
                kind: SfCellKind::Macro,
                intrinsic_delay_ps: 16.5,
                setup_ps: 8.5,
                hold_ps: 5.5,
            },
            metadata: None,
        });

        let cell = updated
            .cell_library
            .cells
            .iter()
            .find(|cell| cell.name == "compound_buf")
            .expect("characterized cell should be inserted");
        assert_eq!(cell.area_um2, 52.0);
        let timing = updated
            .cell_timing_for_cell("compound_buf", SfCellKind::Macro)
            .expect("named macro timing should exist");
        assert_eq!(timing.intrinsic_delay_ps, 16.5);
        assert_eq!(
            updated
                .cell_timing(SfCellKind::Macro)
                .expect("default macro timing should remain")
                .intrinsic_delay_ps,
            14.0
        );
    }

    #[test]
    fn pdk_resolves_characterized_arc_delay_for_pin_pair() {
        let base = Pdk::minimal("test");
        let updated = base.with_characterized_cell(CharacterizedCellLibraryEntry {
            cell: SfCell {
                name: "macro_buf".to_string(),
                kind: SfCellKind::Macro,
                area_um2: 52.0,
                pipeline_stages: 2,
            },
            timing: CellTimingModel {
                kind: SfCellKind::Macro,
                intrinsic_delay_ps: 14.0,
                setup_ps: 8.0,
                hold_ps: 5.0,
            },
            metadata: Some(CharacterizationArtifactMetadata {
                arc_delays: vec![CharacterizationArcDelay {
                    name: "macro_to_sink".to_string(),
                    driver_cell_name: "macro_buf".to_string(),
                    from_port: 0,
                    sink_cell_name: "sink".to_string(),
                    to_port: 0,
                    delay_ps: 37.5,
                }],
                ..CharacterizationArtifactMetadata::default()
            }),
        });

        assert_eq!(
            updated
                .characterized_arc_delay_ps("macro_buf", 0, "sink", 0)
                .expect("arc delay"),
            37.5
        );
    }

    #[test]
    fn characterization_metadata_reports_delay_detail_spread() {
        let metadata = CharacterizationArtifactMetadata {
            delay_details: vec![
                CharacterizationDelayDetail {
                    name: "a".to_string(),
                    delay_ps: 10.0,
                },
                CharacterizationDelayDetail {
                    name: "b".to_string(),
                    delay_ps: 14.0,
                },
            ],
            ..CharacterizationArtifactMetadata::default()
        };

        assert_eq!(metadata.delay_detail_spread_sigma_ps(), 2.0);
    }

    #[test]
    fn pdk_can_merge_characterized_library_bundle() {
        let base = Pdk::minimal("test");
        let bundle = CharacterizedCellLibraryBundle {
            entries: vec![
                CharacterizedCellLibraryEntry {
                    cell: SfCell {
                        name: "macro_a".to_string(),
                        kind: SfCellKind::Macro,
                        area_um2: 40.0,
                        pipeline_stages: 2,
                    },
                    timing: CellTimingModel {
                        kind: SfCellKind::Macro,
                        intrinsic_delay_ps: 18.0,
                        setup_ps: 4.0,
                        hold_ps: 3.0,
                    },
                    metadata: None,
                },
                CharacterizedCellLibraryEntry {
                    cell: SfCell {
                        name: "macro_b".to_string(),
                        kind: SfCellKind::Macro,
                        area_um2: 72.0,
                        pipeline_stages: 3,
                    },
                    timing: CellTimingModel {
                        kind: SfCellKind::Macro,
                        intrinsic_delay_ps: 22.0,
                        setup_ps: 5.0,
                        hold_ps: 4.0,
                    },
                    metadata: Some(CharacterizationArtifactMetadata {
                        waveform_path: Some("out.raw".to_string()),
                        simulated_delay_ps: Some(24.0),
                        sta_derived_delay_ps: Some(22.0),
                        delay_calibration_sigma_ps: 0.7,
                        delay_details: Vec::new(),
                        arc_delays: Vec::new(),
                    }),
                },
            ],
        };
        let serialized = serde_json::to_string(&bundle).expect("bundle should serialize");
        let updated = base
            .with_characterized_library_bundle_json(&serialized)
            .expect("bundle json should deserialize");

        assert_eq!(
            updated
                .cell_timing_for_cell("macro_a", SfCellKind::Macro)
                .expect("macro_a timing")
                .intrinsic_delay_ps,
            18.0
        );
        assert_eq!(
            updated
                .cell_timing_for_cell("macro_b", SfCellKind::Macro)
                .expect("macro_b timing")
                .intrinsic_delay_ps,
            22.0
        );
        let metadata = updated
            .characterization_metadata_for_cell("macro_b")
            .expect("macro_b metadata");
        assert_eq!(metadata.waveform_path.as_deref(), Some("out.raw"));
        assert_eq!(metadata.delay_calibration_sigma_ps, 0.7);
    }

    #[test]
    fn pdk_can_apply_characterized_library_json() {
        let base = Pdk::minimal("test");
        let updated = base
            .with_characterized_library_json(
                r#"{
    "cell": {
        "name": "macro_buf",
        "kind": "Macro",
        "area_um2": 40.0,
        "pipeline_stages": 2
  },
    "timing": {
        "kind": "Macro",
        "intrinsic_delay_ps": 19.0,
        "setup_ps": 4.0,
        "hold_ps": 3.0
  }
}"#,
            )
            .expect("json characterization artifact should deserialize");

        assert_eq!(
            updated
                .cell_timing_for_cell("macro_buf", SfCellKind::Macro)
                .expect("named macro timing should exist")
                .intrinsic_delay_ps,
            19.0
        );
    }

    #[test]
    fn interpolates_interconnect_delay_between_points() {
        let pdk = Pdk::minimal("test");
        let delay = pdk
            .interconnect_delay_ps(InterconnectKind::Jtl, 20.0)
            .expect("timing model must exist");

        assert_eq!(delay, 12.0);
    }

    #[test]
    fn extrapolates_interconnect_delay_past_last_point() {
        let pdk = Pdk::minimal("test");
        let delay = pdk
            .interconnect_delay_ps(InterconnectKind::Ptl, 200.0)
            .expect("timing model must exist");

        assert_eq!(delay, 24.0);
    }
}
