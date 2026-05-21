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
use thiserror::Error;

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
}

pub fn write_ir_json(path: impl AsRef<Path>, netlist: &Netlist) -> Result<(), IoError> {
    let content = serde_json::to_string_pretty(netlist)?;
    fs::write(path, content)?;
    Ok(())
}

pub fn read_ir_json(path: impl AsRef<Path>) -> Result<Netlist, IoError> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn write_pdk_json(path: impl AsRef<Path>, pdk: &Pdk) -> Result<(), IoError> {
    let content = serde_json::to_string_pretty(pdk)?;
    fs::write(path, content)?;
    Ok(())
}

pub fn read_pdk_json(path: impl AsRef<Path>) -> Result<Pdk, IoError> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
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
