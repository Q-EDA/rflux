use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const CHARACTERIZED_ARC_ANY_SINK: &str = "*";

const REQUIRED_CELL_KINDS: [SfCellKind; 7] = [
    SfCellKind::GenericGate,
    SfCellKind::Macro,
    SfCellKind::Splitter,
    SfCellKind::Dff,
    SfCellKind::Jtl,
    SfCellKind::Ptl,
    SfCellKind::Port,
];

const REQUIRED_INTERCONNECT_KINDS: [InterconnectKind; 2] = [InterconnectKind::Jtl, InterconnectKind::Ptl];

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub cells: Vec<SfCell>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellLibraryMetadata {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellLibraryEntry {
    pub name: String,
    pub kind: SfCellKind,
    pub area_um2: f64,
    pub pipeline_stages: u8,
    pub intrinsic_delay_ps: f64,
    pub setup_ps: f64,
    pub hold_ps: f64,
    pub timing_source: String,
    pub has_characterization_metadata: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellLibrarySummary {
    pub cell_count: usize,
    pub kind_count: usize,
    pub kind_counts: BTreeMap<SfCellKind, usize>,
    pub named_timing_count: usize,
    pub kind_timing_count: usize,
    pub missing_timing_count: usize,
    pub characterized_cell_count: usize,
    pub named_timing_cells: Vec<String>,
    pub missing_timing_cells: Vec<String>,
    pub characterized_cells: Vec<String>,
}

impl SfCellLibrary {
    pub fn minimal() -> Self {
        Self {
            name: "minimal-sfq".to_string(),
            version: Some("0.1.0".to_string()),
            source: Some("rflux-minimal".to_string()),
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdkValidationReport {
    pub errors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl PdkValidationReport {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
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

    pub fn cell_library_name(&self) -> &str {
        &self.cell_library.name
    }

    pub fn cell_library_version(&self) -> Option<&str> {
        self.cell_library.version.as_deref()
    }

    pub fn cell_library_source(&self) -> Option<&str> {
        self.cell_library.source.as_deref()
    }

    pub fn cell_library_metadata(&self) -> CellLibraryMetadata {
        CellLibraryMetadata {
            name: self.cell_library.name.clone(),
            version: self.cell_library.version.clone(),
            source: self.cell_library.source.clone(),
        }
    }

    pub fn cell_library_kinds(&self) -> Vec<SfCellKind> {
        let mut kinds = Vec::new();
        for cell in &self.cell_library.cells {
            if !kinds.contains(&cell.kind) {
                kinds.push(cell.kind);
            }
        }
        kinds
    }

    pub fn cell_library_entries(&self) -> Vec<CellLibraryEntry> {
        self.cell_library
            .cells
            .iter()
            .map(|cell| self.cell_library_entry_for_cell(cell))
            .collect()
    }

    pub fn cell_library_summary(&self) -> CellLibrarySummary {
        let entries = self.cell_library_entries();
        let mut kind_counts = BTreeMap::new();
        for entry in &entries {
            *kind_counts.entry(entry.kind).or_insert(0) += 1;
        }
        let named_timing_cells = entries
            .iter()
            .filter(|entry| entry.timing_source == "named")
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();
        let missing_timing_cells = entries
            .iter()
            .filter(|entry| entry.timing_source == "missing")
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();
        let characterized_cells = entries
            .iter()
            .filter(|entry| entry.has_characterization_metadata)
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();
        CellLibrarySummary {
            cell_count: entries.len(),
            kind_count: self.cell_library_kinds().len(),
            kind_counts,
            named_timing_count: named_timing_cells.len(),
            kind_timing_count: entries
                .iter()
                .filter(|entry| entry.timing_source == "kind")
                .count(),
            missing_timing_count: missing_timing_cells.len(),
            characterized_cell_count: characterized_cells.len(),
            named_timing_cells,
            missing_timing_cells,
            characterized_cells,
        }
    }

    pub fn cell_library_entries_by_kind(&self, kind: SfCellKind) -> Vec<CellLibraryEntry> {
        self.cell_library
            .cells
            .iter()
            .filter(|cell| cell.kind == kind)
            .map(|cell| self.cell_library_entry_for_cell(cell))
            .collect()
    }

    pub fn cell_library_entry(&self, cell_name: &str) -> Option<CellLibraryEntry> {
        self.cell_library
            .find_by_name(cell_name)
            .map(|cell| self.cell_library_entry_for_cell(cell))
    }

    fn cell_library_entry_for_cell(&self, cell: &SfCell) -> CellLibraryEntry {
        let named_timing = self
            .named_cell_timing
            .iter()
            .find(|model| model.cell_name == cell.name);
        let timing = named_timing
            .map(|model| &model.timing)
            .or_else(|| self.cell_timing(cell.kind));
        CellLibraryEntry {
            name: cell.name.clone(),
            kind: cell.kind,
            area_um2: cell.area_um2,
            pipeline_stages: cell.pipeline_stages,
            intrinsic_delay_ps: timing
                .map(|timing| timing.intrinsic_delay_ps)
                .unwrap_or_default(),
            setup_ps: timing.map(|timing| timing.setup_ps).unwrap_or_default(),
            hold_ps: timing.map(|timing| timing.hold_ps).unwrap_or_default(),
            timing_source: if named_timing.is_some() {
                "named".to_string()
            } else if timing.is_some() {
                "kind".to_string()
            } else {
                "missing".to_string()
            },
            has_characterization_metadata: self
                .characterization_metadata_for_cell(&cell.name)
                .is_some(),
        }
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
            .or_else(|| {
                metadata.arc_delays.iter().find(|arc| {
                    arc.driver_cell_name == driver_cell_name
                        && arc.from_port == from_port
                        && arc.sink_cell_name == CHARACTERIZED_ARC_ANY_SINK
                        && arc.to_port == to_port
                })
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

    pub fn validate(&self) -> PdkValidationReport {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if self.name.trim().is_empty() {
            errors.push("pdk.name must not be empty".to_string());
        }
        if self.metal_layers == 0 {
            errors.push("pdk.metal_layers must be greater than zero".to_string());
        }
        if self.cell_library.cells.is_empty() {
            errors.push("pdk.cell_library must contain at least one cell".to_string());
        }
        if self.cell_library.name.trim().is_empty() {
            errors.push("pdk.cell_library.name must not be empty".to_string());
        }
        if self
            .cell_library
            .version
            .as_deref()
            .is_none_or(|version| version.trim().is_empty())
        {
            warnings.push("pdk.cell_library.version is not set".to_string());
        }
        if self
            .cell_library
            .source
            .as_deref()
            .is_none_or(|source| source.trim().is_empty())
        {
            warnings.push("pdk.cell_library.source is not set".to_string());
        }

        let mut seen_cell_names = std::collections::BTreeSet::new();
        for cell in &self.cell_library.cells {
            if cell.name.trim().is_empty() {
                errors.push("pdk.cell_library contains a cell with an empty name".to_string());
            }
            if !seen_cell_names.insert(cell.name.clone()) {
                errors.push(format!(
                    "pdk.cell_library contains duplicate cell name '{}'",
                    cell.name
                ));
            }
            if cell.area_um2 < 0.0 {
                errors.push(format!(
                    "pdk.cell_library cell '{}' has negative area_um2 {}",
                    cell.name, cell.area_um2
                ));
            }
        }

        for required_kind in REQUIRED_CELL_KINDS {
            if !self.cell_library.cells.iter().any(|cell| cell.kind == required_kind) {
                errors.push(format!(
                    "pdk.cell_library is missing required cell kind {:?}",
                    required_kind
                ));
            }
        }

        let mut seen_timing_kinds = std::collections::HashSet::new();
        for timing in &self.cell_timing {
            if !seen_timing_kinds.insert(timing.kind) {
                errors.push(format!(
                    "pdk.cell_timing contains duplicate timing entry for kind {:?}",
                    timing.kind
                ));
            }
            if timing.intrinsic_delay_ps < 0.0 {
                errors.push(format!(
                    "pdk.cell_timing {:?} has negative intrinsic_delay_ps {}",
                    timing.kind, timing.intrinsic_delay_ps
                ));
            }
            if timing.setup_ps < 0.0 {
                errors.push(format!(
                    "pdk.cell_timing {:?} has negative setup_ps {}",
                    timing.kind, timing.setup_ps
                ));
            }
            if timing.hold_ps < 0.0 {
                errors.push(format!(
                    "pdk.cell_timing {:?} has negative hold_ps {}",
                    timing.kind, timing.hold_ps
                ));
            }
        }

        for required_kind in REQUIRED_CELL_KINDS {
            if !self.cell_timing.iter().any(|timing| timing.kind == required_kind) {
                errors.push(format!(
                    "pdk.cell_timing is missing required timing entry for kind {:?}",
                    required_kind
                ));
            }
        }

        let mut seen_named_timing_cells = std::collections::BTreeSet::new();
        for timing in &self.named_cell_timing {
            if !seen_named_timing_cells.insert(timing.cell_name.clone()) {
                errors.push(format!(
                    "pdk.named_cell_timing contains duplicate entry for cell '{}'",
                    timing.cell_name
                ));
            }
            let Some(cell) = self.cell_library.find_by_name(&timing.cell_name) else {
                errors.push(format!(
                    "pdk.named_cell_timing references unknown cell '{}'",
                    timing.cell_name
                ));
                continue;
            };
            if timing.timing.kind != cell.kind {
                errors.push(format!(
                    "pdk.named_cell_timing for '{}' uses kind {:?}, but the cell library declares {:?}",
                    timing.cell_name, timing.timing.kind, cell.kind
                ));
            }
            if timing.timing.intrinsic_delay_ps < 0.0 {
                errors.push(format!(
                    "pdk.named_cell_timing '{}' has negative intrinsic_delay_ps {}",
                    timing.cell_name, timing.timing.intrinsic_delay_ps
                ));
            }
            if timing.timing.setup_ps < 0.0 {
                errors.push(format!(
                    "pdk.named_cell_timing '{}' has negative setup_ps {}",
                    timing.cell_name, timing.timing.setup_ps
                ));
            }
            if timing.timing.hold_ps < 0.0 {
                errors.push(format!(
                    "pdk.named_cell_timing '{}' has negative hold_ps {}",
                    timing.cell_name, timing.timing.hold_ps
                ));
            }
        }

        let mut seen_metadata_cells = std::collections::BTreeSet::new();
        for metadata in &self.characterized_cell_metadata {
            if !seen_metadata_cells.insert(metadata.cell_name.clone()) {
                errors.push(format!(
                    "pdk.characterized_cell_metadata contains duplicate entry for cell '{}'",
                    metadata.cell_name
                ));
            }
            if self.cell_library.find_by_name(&metadata.cell_name).is_none() {
                errors.push(format!(
                    "pdk.characterized_cell_metadata references unknown cell '{}'",
                    metadata.cell_name
                ));
            }
            if metadata.metadata.delay_calibration_sigma_ps < 0.0 {
                errors.push(format!(
                    "pdk.characterized_cell_metadata '{}' has negative delay_calibration_sigma_ps {}",
                    metadata.cell_name, metadata.metadata.delay_calibration_sigma_ps
                ));
            }
            if metadata
                .metadata
                .simulated_delay_ps
                .is_some_and(|delay| delay < 0.0)
            {
                errors.push(format!(
                    "pdk.characterized_cell_metadata '{}' has negative simulated_delay_ps",
                    metadata.cell_name
                ));
            }
            if metadata
                .metadata
                .sta_derived_delay_ps
                .is_some_and(|delay| delay < 0.0)
            {
                errors.push(format!(
                    "pdk.characterized_cell_metadata '{}' has negative sta_derived_delay_ps",
                    metadata.cell_name
                ));
            }
            for detail in &metadata.metadata.delay_details {
                if detail.delay_ps < 0.0 {
                    errors.push(format!(
                        "pdk.characterized_cell_metadata '{}' delay detail '{}' has negative delay_ps {}",
                        metadata.cell_name, detail.name, detail.delay_ps
                    ));
                }
            }
            if metadata.metadata.arc_delays.is_empty() {
                warnings.push(format!(
                    "pdk.characterized_cell_metadata '{}' has no arc_delays; STA will use kind-level or named-cell timing fallback",
                    metadata.cell_name
                ));
            }
            let mut seen_arc_keys = std::collections::BTreeSet::new();
            for arc in &metadata.metadata.arc_delays {
                let arc_key = (
                    arc.driver_cell_name.clone(),
                    arc.from_port,
                    arc.sink_cell_name.clone(),
                    arc.to_port,
                );
                if !seen_arc_keys.insert(arc_key) {
                    errors.push(format!(
                        "pdk.characterized_cell_metadata '{}' contains duplicate arc signature {}:{} -> {}:{}",
                        metadata.cell_name,
                        arc.driver_cell_name,
                        arc.from_port,
                        arc.sink_cell_name,
                        arc.to_port
                    ));
                }
                if arc.delay_ps < 0.0 {
                    errors.push(format!(
                        "pdk.characterized_cell_metadata '{}' arc '{}' has negative delay_ps {}",
                        metadata.cell_name, arc.name, arc.delay_ps
                    ));
                }
                if self.cell_library.find_by_name(&arc.driver_cell_name).is_none() {
                    warnings.push(format!(
                        "pdk.characterized_cell_metadata '{}' arc '{}' references unknown driver cell '{}'",
                        metadata.cell_name, arc.name, arc.driver_cell_name
                    ));
                }
                if arc.sink_cell_name != CHARACTERIZED_ARC_ANY_SINK
                    && self.cell_library.find_by_name(&arc.sink_cell_name).is_none()
                {
                    warnings.push(format!(
                        "pdk.characterized_cell_metadata '{}' arc '{}' references unknown sink cell '{}'",
                        metadata.cell_name, arc.name, arc.sink_cell_name
                    ));
                }
            }
        }

        for range in &self.ptl_forbidden_ranges {
            if range.min_um < 0.0 || range.max_um < 0.0 {
                errors.push(format!(
                    "pdk.ptl_forbidden_ranges contains a negative range [{}, {}]",
                    range.min_um, range.max_um
                ));
            }
            if range.min_um > range.max_um {
                errors.push(format!(
                    "pdk.ptl_forbidden_ranges has inverted range [{}, {}]",
                    range.min_um, range.max_um
                ));
            }
        }

        let mut seen_interconnect_kinds = std::collections::HashSet::new();
        for model in &self.interconnect_timing {
            if !seen_interconnect_kinds.insert(model.kind) {
                errors.push(format!(
                    "pdk.interconnect_timing contains duplicate timing model for kind {:?}",
                    model.kind
                ));
            }
            if model.points.is_empty() {
                errors.push(format!(
                    "pdk.interconnect_timing {:?} must contain at least one point",
                    model.kind
                ));
                continue;
            }
            let mut previous_length = None;
            for point in &model.points {
                if point.length_um < 0.0 {
                    errors.push(format!(
                        "pdk.interconnect_timing {:?} has negative length_um {}",
                        model.kind, point.length_um
                    ));
                }
                if point.delay_ps < 0.0 {
                    errors.push(format!(
                        "pdk.interconnect_timing {:?} has negative delay_ps {}",
                        model.kind, point.delay_ps
                    ));
                }
                if let Some(previous_length) = previous_length {
                    if point.length_um <= previous_length {
                        errors.push(format!(
                            "pdk.interconnect_timing {:?} points must be strictly increasing by length_um",
                            model.kind
                        ));
                        break;
                    }
                }
                previous_length = Some(point.length_um);
            }
        }

        for required_kind in REQUIRED_INTERCONNECT_KINDS {
            if !self
                .interconnect_timing
                .iter()
                .any(|model| model.kind == required_kind)
            {
                errors.push(format!(
                    "pdk.interconnect_timing is missing required timing model for kind {:?}",
                    required_kind
                ));
            }
        }

        PdkValidationReport { errors, warnings }
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
    fn pdk_resolves_characterized_arc_delay_with_wildcard_sink_fallback() {
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
                    name: "macro_output_port_0".to_string(),
                    driver_cell_name: "macro_buf".to_string(),
                    from_port: 0,
                    sink_cell_name: CHARACTERIZED_ARC_ANY_SINK.to_string(),
                    to_port: 0,
                    delay_ps: 29.0,
                }],
                ..CharacterizationArtifactMetadata::default()
            }),
        });

        assert_eq!(
            updated
                .characterized_arc_delay_ps("macro_buf", 0, "consumer_sink", 0)
                .expect("arc delay"),
            29.0
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
    fn pdk_lists_cell_library_entries_with_effective_timing() {
        let pdk = Pdk::minimal("test");
        let entries = pdk.cell_library_entries();
        let gate = entries
            .iter()
            .find(|entry| entry.name == "sfq_gate")
            .expect("minimal PDK should expose sfq_gate");

        assert_eq!(pdk.cell_library_name(), "minimal-sfq");
        assert_eq!(pdk.cell_library_version(), Some("0.1.0"));
        assert_eq!(pdk.cell_library_source(), Some("rflux-minimal"));
        assert_eq!(
            pdk.cell_library_metadata(),
            CellLibraryMetadata {
                name: "minimal-sfq".to_string(),
                version: Some("0.1.0".to_string()),
                source: Some("rflux-minimal".to_string()),
            }
        );
        assert_eq!(pdk.cell_library_kinds().len(), REQUIRED_CELL_KINDS.len());
        let summary = pdk.cell_library_summary();
        assert_eq!(summary.cell_count, REQUIRED_CELL_KINDS.len());
        assert_eq!(summary.kind_count, REQUIRED_CELL_KINDS.len());
        assert_eq!(
            summary.kind_counts.get(&SfCellKind::GenericGate).copied(),
            Some(1)
        );
        assert_eq!(
            summary.kind_counts.get(&SfCellKind::Macro).copied(),
            Some(1)
        );
        assert_eq!(summary.named_timing_count, 0);
        assert_eq!(summary.kind_timing_count, REQUIRED_CELL_KINDS.len());
        assert_eq!(summary.missing_timing_count, 0);
        assert_eq!(summary.characterized_cell_count, 0);
        assert!(summary.named_timing_cells.is_empty());
        assert!(summary.missing_timing_cells.is_empty());
        assert!(summary.characterized_cells.is_empty());
        assert!(entries.len() >= REQUIRED_CELL_KINDS.len());
        assert_eq!(gate.kind, SfCellKind::GenericGate);
        assert_eq!(gate.timing_source, "kind");
        assert_eq!(gate.intrinsic_delay_ps, 8.0);
        assert!(!gate.has_characterization_metadata);

        let gate_by_name = pdk
            .cell_library_entry("sfq_gate")
            .expect("sfq_gate should be queryable by name");
        assert_eq!(gate_by_name, *gate);
        assert!(pdk.cell_library_entry("missing").is_none());

        let macros = pdk.cell_library_entries_by_kind(SfCellKind::Macro);
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].name, "sfq_macro");
    }

    #[test]
    fn pdk_accepts_legacy_cell_library_without_metadata() {
        let mut payload = serde_json::to_value(Pdk::minimal("legacy"))
            .expect("minimal PDK should serialize to JSON value");
        let cell_library = payload
            .get_mut("cell_library")
            .and_then(|value| value.as_object_mut())
            .expect("minimal PDK should contain cell_library object");
        cell_library.remove("version");
        cell_library.remove("source");

        let legacy = Pdk::from_json(&payload.to_string()).expect("legacy PDK should deserialize");

        assert_eq!(legacy.cell_library_name(), "minimal-sfq");
        assert_eq!(legacy.cell_library_version(), None);
        assert_eq!(legacy.cell_library_source(), None);
        assert_eq!(legacy.cell_library_summary().cell_count, REQUIRED_CELL_KINDS.len());
        let report = legacy.validate();
        assert!(report.is_ok());
        assert_eq!(
            report.warnings,
            vec![
                "pdk.cell_library.version is not set".to_string(),
                "pdk.cell_library.source is not set".to_string()
            ]
        );
    }

    #[test]
    fn pdk_cell_library_entries_prefer_named_characterized_timing() {
        let updated = Pdk::minimal("test").with_characterized_cell(CharacterizedCellLibraryEntry {
            cell: SfCell {
                name: "compound_buf".to_string(),
                kind: SfCellKind::Macro,
                area_um2: 52.0,
                pipeline_stages: 2,
            },
            timing: CellTimingModel {
                kind: SfCellKind::Macro,
                intrinsic_delay_ps: 17.5,
                setup_ps: 8.5,
                hold_ps: 5.5,
            },
            metadata: Some(CharacterizationArtifactMetadata {
                waveform_path: Some("compound.raw".to_string()),
                ..CharacterizationArtifactMetadata::default()
            }),
        });

        let entry = updated
            .cell_library_entries()
            .into_iter()
            .find(|entry| entry.name == "compound_buf")
            .expect("characterized cell should be listed");

        assert_eq!(entry.kind, SfCellKind::Macro);
        assert_eq!(entry.area_um2, 52.0);
        assert_eq!(entry.pipeline_stages, 2);
        assert_eq!(entry.intrinsic_delay_ps, 17.5);
        assert_eq!(entry.timing_source, "named");
        assert!(entry.has_characterization_metadata);

        let summary = updated.cell_library_summary();
        assert_eq!(summary.kind_counts.get(&SfCellKind::Macro).copied(), Some(2));
        assert_eq!(summary.named_timing_count, 1);
        assert_eq!(summary.characterized_cell_count, 1);
        assert_eq!(summary.missing_timing_count, 0);
        assert_eq!(summary.named_timing_cells, vec!["compound_buf"]);
        assert_eq!(summary.characterized_cells, vec!["compound_buf"]);
        assert!(summary.missing_timing_cells.is_empty());
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

    #[test]
    fn minimal_pdk_validates_cleanly() {
        let report = Pdk::minimal("test").validate();

        assert!(report.is_ok());
        assert!(report.errors.is_empty());
    }

    #[test]
    fn pdk_validation_reports_structural_errors() {
        let mut pdk = Pdk::minimal(" ");
        pdk.metal_layers = 0;
        pdk.cell_library.cells.push(SfCell {
            name: "sfq_gate".to_string(),
            kind: SfCellKind::GenericGate,
            area_um2: -1.0,
            pipeline_stages: 1,
        });
        pdk.named_cell_timing.push(NamedCellTimingModel {
            cell_name: "missing_cell".to_string(),
            timing: CellTimingModel {
                kind: SfCellKind::Macro,
                intrinsic_delay_ps: 1.0,
                setup_ps: 1.0,
                hold_ps: 1.0,
            },
        });
        pdk.ptl_forbidden_ranges.push(LengthRange {
            min_um: 10.0,
            max_um: 5.0,
        });
        pdk.interconnect_timing.push(InterconnectTimingModel {
            kind: InterconnectKind::Jtl,
            points: vec![
                TimingPoint {
                    length_um: 5.0,
                    delay_ps: 1.0,
                },
                TimingPoint {
                    length_um: 5.0,
                    delay_ps: 2.0,
                },
            ],
        });

        let report = pdk.validate();

        assert!(!report.is_ok());
        assert!(report
            .errors
            .iter()
            .any(|error| error == "pdk.name must not be empty"));
        assert!(report
            .errors
            .iter()
            .any(|error| error == "pdk.metal_layers must be greater than zero"));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("duplicate cell name 'sfq_gate'")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("references unknown cell 'missing_cell'")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("inverted range [10, 5]")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("duplicate timing model for kind Jtl")));
    }

    #[test]
    fn pdk_validation_reports_missing_required_coverage() {
        let mut pdk = Pdk::minimal("test");
        pdk.cell_library.cells.retain(|cell| cell.kind != SfCellKind::Dff);
        pdk.cell_timing.retain(|timing| timing.kind != SfCellKind::Splitter);
        pdk.interconnect_timing
            .retain(|timing| timing.kind != InterconnectKind::Ptl);

        let report = pdk.validate();

        assert!(!report.is_ok());
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("missing required cell kind Dff")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("missing required timing entry for kind Splitter")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("missing required timing model for kind Ptl")));
    }

    #[test]
    fn pdk_validation_reports_named_timing_and_metadata_consistency_errors() {
        let mut pdk = Pdk::minimal("test");
        pdk.named_cell_timing.push(NamedCellTimingModel {
            cell_name: "sfq_gate".to_string(),
            timing: CellTimingModel {
                kind: SfCellKind::GenericGate,
                intrinsic_delay_ps: -1.0,
                setup_ps: 1.0,
                hold_ps: 1.0,
            },
        });
        pdk.named_cell_timing.push(NamedCellTimingModel {
            cell_name: "sfq_gate".to_string(),
            timing: CellTimingModel {
                kind: SfCellKind::GenericGate,
                intrinsic_delay_ps: 2.0,
                setup_ps: 1.0,
                hold_ps: 1.0,
            },
        });
        pdk.characterized_cell_metadata.push(NamedCharacterizationMetadata {
            cell_name: "sfq_macro".to_string(),
            metadata: CharacterizationArtifactMetadata {
                delay_calibration_sigma_ps: -0.1,
                delay_details: vec![CharacterizationDelayDetail {
                    name: "bad-detail".to_string(),
                    delay_ps: -3.0,
                }],
                arc_delays: vec![CharacterizationArcDelay {
                    name: "bad-arc".to_string(),
                    driver_cell_name: "sfq_macro".to_string(),
                    from_port: 0,
                    sink_cell_name: "sink".to_string(),
                    to_port: 0,
                    delay_ps: -4.0,
                }, CharacterizationArcDelay {
                    name: "bad-arc-duplicate".to_string(),
                    driver_cell_name: "sfq_macro".to_string(),
                    from_port: 0,
                    sink_cell_name: "sink".to_string(),
                    to_port: 0,
                    delay_ps: 2.0,
                }],
                ..CharacterizationArtifactMetadata::default()
            },
        });
        pdk.characterized_cell_metadata.push(NamedCharacterizationMetadata {
            cell_name: "sfq_macro".to_string(),
            metadata: CharacterizationArtifactMetadata::default(),
        });

        let report = pdk.validate();

        assert!(!report.is_ok());
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("duplicate entry for cell 'sfq_gate'")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("negative intrinsic_delay_ps -1")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("duplicate entry for cell 'sfq_macro'")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("negative delay_calibration_sigma_ps -0.1")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("delay detail 'bad-detail' has negative delay_ps -3")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("arc 'bad-arc' has negative delay_ps -4")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("duplicate arc signature sfq_macro:0 -> sink:0")));
    }

    #[test]
    fn pdk_validation_reports_characterization_advisory_warnings() {
        let mut pdk = Pdk::minimal("test");
        pdk.characterized_cell_metadata.push(NamedCharacterizationMetadata {
            cell_name: "sfq_macro".to_string(),
            metadata: CharacterizationArtifactMetadata {
                arc_delays: vec![CharacterizationArcDelay {
                    name: "unknown-sink".to_string(),
                    driver_cell_name: "sfq_macro".to_string(),
                    from_port: 0,
                    sink_cell_name: "missing_sink".to_string(),
                    to_port: 0,
                    delay_ps: 2.0,
                }],
                ..CharacterizationArtifactMetadata::default()
            },
        });

        let report = pdk.validate();

        assert!(report.is_ok());
        assert!(report.errors.is_empty());
        assert!(report.warnings.iter().any(|warning| warning
            .contains("arc 'unknown-sink' references unknown sink cell 'missing_sink'")));
    }
}
