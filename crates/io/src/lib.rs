use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::io::BufWriter;
use std::path::Path;

use libreda_db::chip::Chip;
use libreda_db::prelude::HierarchyBase;
use libreda_lefdef::def_ast::DEF;
use libreda_lefdef::def_parser::read_def_bytes;
use libreda_lefdef::def_writer::write_def;
use libreda_lefdef::export::export_db_to_def;
use libreda_lefdef::export::DEFExportOptions;
use libreda_lefdef::import::import_def_into_db;
use libreda_lefdef::import::lef_to_db;
use libreda_lefdef::import::DEFImportOptions;
use libreda_lefdef::lef_ast::LEF;
use libreda_lefdef::lef_parser::read_lef_bytes;
use rflux_ir::Netlist;
use rflux_tech::Pdk;
use serde_json::{json, Value};
use thiserror::Error;

const IR_JSON_SCHEMA_VERSION: u64 = 1;
const PDK_JSON_SCHEMA_VERSION: u64 = 1;
const IR_JSON_KIND: &str = "rflux_ir_netlist";
const PDK_JSON_KIND: &str = "rflux_pdk";

#[derive(Debug, Error)]
pub enum IoError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("lef/def error: {0}")]
    LefDef(String),
    #[error("top cell not found")]
    TopCellNotFound,
    #[error("lef writing is not supported by libreda-lefdef")]
    LefWriteUnsupported,
    #[error("unsupported {kind} schema version {version}")]
    UnsupportedSchemaVersion { kind: &'static str, version: u64 },
    #[error("invalid {kind} JSON envelope: {detail}")]
    InvalidJsonEnvelope { kind: &'static str, detail: &'static str },
    #[error("expected {expected_kind} JSON envelope, found {found_kind}")]
    UnexpectedJsonKind {
        expected_kind: &'static str,
        found_kind: String,
    },
}

impl IoError {
    pub fn code(&self) -> &'static str {
        match self {
            IoError::Io(_) => "RFLOW-INPUT-001",
            IoError::Json(_) => "RFLOW-INPUT-002",
            IoError::LefDef(_) => "RFLOW-INPUT-002",
            IoError::TopCellNotFound => "RFLOW-INPUT-002",
            IoError::LefWriteUnsupported => "RFLOW-LIMIT-001",
            IoError::UnsupportedSchemaVersion { .. } => "RFLOW-SCHEMA-001",
            IoError::InvalidJsonEnvelope { .. } => "RFLOW-SCHEMA-002",
            IoError::UnexpectedJsonKind { .. } => "RFLOW-SCHEMA-003",
        }
    }

    pub fn suggestion(&self) -> &'static str {
        match self {
            IoError::Io(_) => "Check that the input path exists and is readable, then retry.",
            IoError::Json(_) => "Validate the JSON syntax and field types against the current file contract.",
            IoError::LefDef(_) => "Validate the LEF/DEF syntax and ensure the file matches the supported subset.",
            IoError::TopCellNotFound => {
                "Specify a valid top cell name or ensure the design has a unique top-level cell."
            }
            IoError::LefWriteUnsupported => {
                "Use DEF export for now; LEF writing is not part of the supported output surface yet."
            }
            IoError::UnsupportedSchemaVersion { .. } => {
                "Regenerate the file with the current toolchain or run a compatible rflux version."
            }
            IoError::InvalidJsonEnvelope { .. } => {
                "Ensure the JSON envelope contains schema_version, kind, and payload."
            }
            IoError::UnexpectedJsonKind { .. } => {
                "Use the correct file type for this command, or regenerate the file with the matching writer."
            }
        }
    }
}

pub fn write_ir_json(path: impl AsRef<Path>, netlist: &Netlist) -> Result<(), IoError> {
    let content = serde_json::to_string_pretty(&json!({
        "schema_version": IR_JSON_SCHEMA_VERSION,
        "kind": IR_JSON_KIND,
        "payload": netlist,
    }))?;
    fs::write(path, content)?;
    Ok(())
}

pub fn read_ir_json(path: impl AsRef<Path>) -> Result<Netlist, IoError> {
    let content = fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&content)?;
    if let Some(payload) = extract_versioned_payload(&json, IR_JSON_KIND, IR_JSON_SCHEMA_VERSION)? {
        return Ok(serde_json::from_value(payload)?);
    }
    Ok(serde_json::from_value(json)?)
}

pub fn write_pdk_json(path: impl AsRef<Path>, pdk: &Pdk) -> Result<(), IoError> {
    let content = serde_json::to_string_pretty(&json!({
        "schema_version": PDK_JSON_SCHEMA_VERSION,
        "kind": PDK_JSON_KIND,
        "payload": pdk,
    }))?;
    fs::write(path, content)?;
    Ok(())
}

pub fn read_pdk_json(path: impl AsRef<Path>) -> Result<Pdk, IoError> {
    let content = fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&content)?;
    if let Some(payload) = extract_versioned_payload(&json, PDK_JSON_KIND, PDK_JSON_SCHEMA_VERSION)? {
        return Ok(serde_json::from_value(payload)?);
    }
    Ok(serde_json::from_value(json)?)
}

fn extract_versioned_payload(
    json: &Value,
    expected_kind: &'static str,
    expected_schema_version: u64,
) -> Result<Option<Value>, IoError> {
    let Some(object) = json.as_object() else {
        return Ok(None);
    };

    let looks_like_envelope = object.contains_key("schema_version")
        || object.contains_key("kind")
        || object.contains_key("payload");
    if !looks_like_envelope {
        return Ok(None);
    }

    let schema_version = object
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or(IoError::InvalidJsonEnvelope {
            kind: expected_kind,
            detail: "missing schema_version",
        })?;
    if schema_version != expected_schema_version {
        return Err(IoError::UnsupportedSchemaVersion {
            kind: expected_kind,
            version: schema_version,
        });
    }

    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or(IoError::InvalidJsonEnvelope {
            kind: expected_kind,
            detail: "missing kind",
        })?;
    if kind != expected_kind {
        return Err(IoError::UnexpectedJsonKind {
            expected_kind,
            found_kind: kind.to_string(),
        });
    }

    object
        .get("payload")
        .cloned()
        .ok_or(IoError::InvalidJsonEnvelope {
            kind: expected_kind,
            detail: "missing payload",
        })
        .map(Some)
}

pub fn read_lef(path: impl AsRef<Path>) -> Result<LEF, IoError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    read_lef_bytes(&mut reader).map_err(|e| IoError::LefDef(e.to_string()))
}

pub fn read_def(path: impl AsRef<Path>) -> Result<DEF, IoError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    read_def_bytes(&mut reader).map_err(|e| IoError::LefDef(e.to_string()))
}

pub fn read_lef_to_chip(path: impl AsRef<Path>) -> Result<Chip, IoError> {
    let lef = read_lef(path)?;
    lef_to_db::<Chip, i32>(&lef).map_err(|e| IoError::LefDef(e.to_string()))
}

pub fn import_def_into_chip(
    def_path: impl AsRef<Path>,
    lef: Option<&LEF>,
    chip: &mut Chip,
) -> Result<(), IoError> {
    let def = read_def(def_path)?;
    let options = DEFImportOptions::<Chip>::default();
    import_def_into_db::<Chip, i32>(&options, lef, &def, chip)
        .map_err(|e| IoError::LefDef(e.to_string()))
}

pub fn read_lef_def_to_chip(
    lef_path: impl AsRef<Path>,
    def_path: impl AsRef<Path>,
) -> Result<Chip, IoError> {
    let lef = read_lef(lef_path)?;
    let mut chip = lef_to_db::<Chip, i32>(&lef).map_err(|e| IoError::LefDef(e.to_string()))?;
    import_def_into_chip(def_path, Some(&lef), &mut chip)?;
    Ok(chip)
}

pub fn write_def_from_chip(
    path: impl AsRef<Path>,
    chip: &Chip,
    top_cell_name: Option<&str>,
) -> Result<(), IoError> {
    let top_cell = if let Some(name) = top_cell_name {
        chip.cell_by_name(name).ok_or(IoError::TopCellNotFound)?
    } else {
        chip.each_cell()
            .find(|c| chip.num_cell_references(c) == 0)
            .ok_or(IoError::TopCellNotFound)?
    };

    let options = DEFExportOptions::<Chip>::default();
    let mut def = DEF::default();
    export_db_to_def::<Chip, i32>(&options, chip, &top_cell, &mut def)
        .map_err(|e| IoError::LefDef(e.to_string()))?;

    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    write_def(&mut writer, &def).map_err(|e| IoError::LefDef(e.to_string()))
}

pub fn write_lef_from_chip(_path: impl AsRef<Path>, _chip: &Chip) -> Result<(), IoError> {
    Err(IoError::LefWriteUnsupported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    use rflux_ir::NodeKind;

    fn unique_test_path(name: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        env::temp_dir().join(format!("rflux-io-{name}-{stamp}.json"))
    }

    #[test]
    fn write_ir_json_wraps_payload_with_schema_metadata() {
        let path = unique_test_path("ir-envelope");
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "in");

        write_ir_json(&path, &netlist).expect("ir json should write");
        let raw = fs::read_to_string(&path).expect("ir json should exist");
        let json: Value = serde_json::from_str(&raw).expect("ir json should parse");

        assert_eq!(json["schema_version"], IR_JSON_SCHEMA_VERSION);
        assert_eq!(json["kind"], IR_JSON_KIND);
        assert!(json.get("payload").is_some());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_ir_json_accepts_legacy_unversioned_payload() {
        let path = unique_test_path("ir-legacy");
        let mut netlist = Netlist::new();
        netlist.add_node(NodeKind::Port, "legacy");
        fs::write(
            &path,
            serde_json::to_string_pretty(&netlist).expect("legacy netlist should serialize"),
        )
        .expect("legacy ir json should write");

        let loaded = read_ir_json(&path).expect("legacy ir json should remain readable");

        assert_eq!(loaded.node_count(), 1);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_ir_json_rejects_unsupported_schema_version() {
        let path = unique_test_path("ir-bad-version");
        fs::write(
            &path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 99,
                "kind": IR_JSON_KIND,
                "payload": Netlist::new(),
            }))
            .expect("bad ir json should serialize"),
        )
        .expect("bad ir json should write");

        let error = read_ir_json(&path).expect_err("unsupported schema version should be rejected");

        assert!(error.to_string().contains("unsupported rflux_ir_netlist schema version 99"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn write_and_read_pdk_json_roundtrip_with_schema_metadata() {
        let path = unique_test_path("pdk-envelope");
        let pdk = Pdk::minimal("minimal-sfq");

        write_pdk_json(&path, &pdk).expect("pdk json should write");
        let raw = fs::read_to_string(&path).expect("pdk json should exist");
        let json: Value = serde_json::from_str(&raw).expect("pdk json should parse");
        let loaded = read_pdk_json(&path).expect("pdk json should roundtrip");

        assert_eq!(json["schema_version"], PDK_JSON_SCHEMA_VERSION);
        assert_eq!(json["kind"], PDK_JSON_KIND);
        assert_eq!(
            serde_json::to_value(&loaded).expect("loaded pdk should serialize"),
            serde_json::to_value(&pdk).expect("pdk should serialize")
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn unsupported_schema_version_maps_to_schema_error_code() {
        let error = IoError::UnsupportedSchemaVersion {
            kind: IR_JSON_KIND,
            version: 2,
        };

        assert_eq!(error.code(), "RFLOW-SCHEMA-001");
        assert!(error.suggestion().contains("Regenerate the file"));
    }

    #[test]
    fn invalid_json_envelope_maps_to_schema_contract_error_code() {
        let error = IoError::InvalidJsonEnvelope {
            kind: PDK_JSON_KIND,
            detail: "missing payload",
        };

        assert_eq!(error.code(), "RFLOW-SCHEMA-002");
        assert!(error.suggestion().contains("schema_version, kind, and payload"));
    }
}
