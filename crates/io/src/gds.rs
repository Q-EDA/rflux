use std::f64;
use std::io::{self, Write};

use rflux_ir::{Netlist, NodeId, NodeKind};

use crate::IoError;

const RECORD_HEADER_LEN: u16 = 4;

const RT_HEADER: u8 = 0x00;
const RT_BGNLIB: u8 = 0x01;
const RT_LIBNAME: u8 = 0x02;
const RT_UNITS: u8 = 0x03;
const RT_ENDLIB: u8 = 0x04;
const RT_BGNSTR: u8 = 0x05;
const RT_STRNAME: u8 = 0x06;
const RT_ENDSTR: u8 = 0x07;
const RT_BOUNDARY: u8 = 0x08;
const RT_PATH: u8 = 0x09;
const RT_SREF: u8 = 0x0A;
const RT_TEXT: u8 = 0x0C;
const RT_LAYER: u8 = 0x0D;
const RT_DATATYPE: u8 = 0x0E;
const RT_WIDTH: u8 = 0x0F;
const RT_XY: u8 = 0x10;
const RT_ENDEL: u8 = 0x11;
const RT_SNAME: u8 = 0x12;
const RT_TEXTTYPE: u8 = 0x15;
const RT_PRESENTATION: u8 = 0x16;
const RT_STRING: u8 = 0x19;
const RT_STRANS: u8 = 0x1A;
const RT_MAG: u8 = 0x1B;
const RT_ANGLE: u8 = 0x1C;
const RT_BOXTYPE: u8 = 0x2E;
const RT_BOX: u8 = 0x2D;

const DT_NONE: u8 = 0x00;
const DT_BITARRAY: u8 = 0x01;
const DT_U16: u8 = 0x02;
const DT_I32: u8 = 0x03;
const DT_F64: u8 = 0x05;
const DT_STRING: u8 = 0x06;

pub const LAYER_CELLS: u16 = 0;
pub const LAYER_JTL: u16 = 1;
pub const LAYER_PTL: u16 = 2;
pub const LAYER_PORTS: u16 = 10;
pub const LAYER_LABELS: u16 = 100;
pub const LAYER_OUTLINE: u16 = 200;

pub const DATATYPE_DEFAULT: u16 = 0;
pub const DATATYPE_FILL: u16 = 0;

const PRESENTATION_LEFT_BOTTOM: u16 = 0x0000;

pub const DB_UNIT_MICRONS: f64 = 0.001;
pub const USER_UNIT_MICRONS: f64 = 0.001;

pub const CELL_WIDTH_UM: f64 = 20.0;
pub const CELL_HEIGHT_UM: f64 = 12.0;
pub const PORT_RADIUS_UM: f64 = 4.0;
pub const PATH_WIDTH_UM: f64 = 2.0;

pub struct GdsLibrary {
    pub name: String,
    pub structures: Vec<GdsStructure>,
}

pub struct GdsStructure {
    pub name: String,
    pub elements: Vec<GdsElement>,
}

pub enum GdsElement {
    Boundary(GdsBoundary),
    Path(GdsPath),
    Sref(GdsSref),
    Text(GdsText),
    Box(GdsBox),
}

pub struct GdsBoundary {
    pub layer: u16,
    pub datatype: u16,
    pub xy: Vec<(i32, i32)>,
}

pub struct GdsPath {
    pub layer: u16,
    pub datatype: u16,
    pub width: i32,
    pub xy: Vec<(i32, i32)>,
}

pub struct GdsSref {
    pub name: String,
    pub x: i32,
    pub y: i32,
}

pub struct GdsText {
    pub layer: u16,
    pub texttype: u16,
    pub x: i32,
    pub y: i32,
    pub string: String,
}

pub struct GdsBox {
    pub layer: u16,
    pub boxtype: u16,
    pub xy: Vec<(i32, i32)>,
}

struct GdsWriter<W: Write> {
    writer: W,
}

impl<W: Write> GdsWriter<W> {
    fn new(writer: W) -> Self {
        Self { writer }
    }

    fn write_record(&mut self, record_type: u8, data_type: u8, data: &[u8]) -> io::Result<()> {
        let length = RECORD_HEADER_LEN + data.len() as u16;
        self.writer.write_all(&length.to_be_bytes())?;
        self.writer.write_all(&[record_type, data_type])?;
        self.writer.write_all(data)?;
        Ok(())
    }

    fn write_none_record(&mut self, record_type: u8) -> io::Result<()> {
        self.write_record(record_type, DT_NONE, &[])
    }

    fn write_u16_record(&mut self, record_type: u8, value: u16) -> io::Result<()> {
        self.write_record(record_type, DT_U16, &value.to_be_bytes())
    }

    fn write_i32_record(&mut self, record_type: u8, value: i32) -> io::Result<()> {
        self.write_record(record_type, DT_I32, &value.to_be_bytes())
    }

    fn write_f64_record(&mut self, record_type: u8, value: f64) -> io::Result<()> {
        let gds_bytes = f64_to_gds_bytes(value);
        self.write_record(record_type, DT_F64, &gds_bytes)
    }

    fn write_string_record(&mut self, record_type: u8, s: &str) -> io::Result<()> {
        let mut data = s.as_bytes().to_vec();
        if !data.len().is_multiple_of(2) {
            data.push(0);
        }
        if data.is_empty() {
            data.push(0);
            data.push(0);
        }
        self.write_record(record_type, DT_STRING, &data)
    }

    fn write_bitarray_record(&mut self, record_type: u8, value: u16) -> io::Result<()> {
        self.write_record(record_type, DT_BITARRAY, &value.to_be_bytes())
    }

    fn write_library(&mut self, lib: &GdsLibrary) -> io::Result<()> {
        self.write_header()?;
        self.write_bgnlib()?;
        self.write_string_record(RT_LIBNAME, &lib.name)?;
        self.write_units()?;
        for s in &lib.structures {
            self.write_structure(s)?;
        }
        self.write_none_record(RT_ENDLIB)?;
        Ok(())
    }

    fn write_header(&mut self) -> io::Result<()> {
        self.write_u16_record(RT_HEADER, 600)
    }

    fn write_bgnlib(&mut self) -> io::Result<()> {
        let (year, month, day, hour, min, sec) = current_ymdhms();
        let mut data = Vec::with_capacity(24);
        for _ in 0..2 {
            data.extend_from_slice(&year.to_be_bytes());
            data.extend_from_slice(&month.to_be_bytes());
            data.extend_from_slice(&day.to_be_bytes());
            data.extend_from_slice(&hour.to_be_bytes());
            data.extend_from_slice(&min.to_be_bytes());
            data.extend_from_slice(&sec.to_be_bytes());
        }
        self.write_record(RT_BGNLIB, DT_U16, &data)
    }

    fn write_units(&mut self) -> io::Result<()> {
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&f64_to_gds_bytes(USER_UNIT_MICRONS));
        data.extend_from_slice(&f64_to_gds_bytes(DB_UNIT_MICRONS));
        self.write_record(RT_UNITS, DT_F64, &data)
    }

    fn write_structure(&mut self, s: &GdsStructure) -> io::Result<()> {
        self.write_bgnstr()?;
        self.write_string_record(RT_STRNAME, &s.name)?;
        for elem in &s.elements {
            self.write_element(elem)?;
        }
        self.write_none_record(RT_ENDSTR)?;
        Ok(())
    }

    fn write_bgnstr(&mut self) -> io::Result<()> {
        let (year, month, day, hour, min, sec) = current_ymdhms();
        let mut data = Vec::with_capacity(24);
        for _ in 0..2 {
            data.extend_from_slice(&year.to_be_bytes());
            data.extend_from_slice(&month.to_be_bytes());
            data.extend_from_slice(&day.to_be_bytes());
            data.extend_from_slice(&hour.to_be_bytes());
            data.extend_from_slice(&min.to_be_bytes());
            data.extend_from_slice(&sec.to_be_bytes());
        }
        self.write_record(RT_BGNSTR, DT_U16, &data)
    }

    fn write_element(&mut self, elem: &GdsElement) -> io::Result<()> {
        match elem {
            GdsElement::Boundary(b) => self.write_boundary(b),
            GdsElement::Path(p) => self.write_path(p),
            GdsElement::Sref(s) => self.write_sref(s),
            GdsElement::Text(t) => self.write_text(t),
            GdsElement::Box(b) => self.write_box(b),
        }
    }

    fn write_boundary(&mut self, b: &GdsBoundary) -> io::Result<()> {
        self.write_none_record(RT_BOUNDARY)?;
        self.write_u16_record(RT_LAYER, b.layer)?;
        self.write_u16_record(RT_DATATYPE, b.datatype)?;
        self.write_xy(&b.xy)?;
        self.write_none_record(RT_ENDEL)?;
        Ok(())
    }

    fn write_path(&mut self, p: &GdsPath) -> io::Result<()> {
        self.write_none_record(RT_PATH)?;
        self.write_u16_record(RT_LAYER, p.layer)?;
        self.write_u16_record(RT_DATATYPE, p.datatype)?;
        self.write_i32_record(RT_WIDTH, p.width)?;
        self.write_xy(&p.xy)?;
        self.write_none_record(RT_ENDEL)?;
        Ok(())
    }

    fn write_sref(&mut self, s: &GdsSref) -> io::Result<()> {
        self.write_none_record(RT_SREF)?;
        self.write_string_record(RT_SNAME, &s.name)?;
        self.write_bitarray_record(RT_STRANS, 0x8000)?;
        self.write_f64_record(RT_MAG, 1.0)?;
        self.write_f64_record(RT_ANGLE, 0.0)?;
        self.write_xy(&[(s.x, s.y)])?;
        self.write_none_record(RT_ENDEL)?;
        Ok(())
    }

    fn write_text(&mut self, t: &GdsText) -> io::Result<()> {
        self.write_none_record(RT_TEXT)?;
        self.write_u16_record(RT_LAYER, t.layer)?;
        self.write_u16_record(RT_TEXTTYPE, t.texttype)?;
        self.write_bitarray_record(RT_STRANS, 0x8000)?;
        self.write_f64_record(RT_MAG, 0.5)?;
        self.write_f64_record(RT_ANGLE, 0.0)?;
        self.write_u16_record(RT_PRESENTATION, PRESENTATION_LEFT_BOTTOM)?;
        self.write_xy(&[(t.x, t.y)])?;
        self.write_string_record(RT_STRING, &t.string)?;
        self.write_none_record(RT_ENDEL)?;
        Ok(())
    }

    fn write_box(&mut self, b: &GdsBox) -> io::Result<()> {
        self.write_none_record(RT_BOX)?;
        self.write_u16_record(RT_LAYER, b.layer)?;
        self.write_u16_record(RT_BOXTYPE, b.boxtype)?;
        self.write_xy(&b.xy)?;
        self.write_none_record(RT_ENDEL)?;
        Ok(())
    }

    fn write_xy(&mut self, points: &[(i32, i32)]) -> io::Result<()> {
        let mut data = Vec::with_capacity(points.len() * 8);
        for &(x, y) in points {
            data.extend_from_slice(&x.to_be_bytes());
            data.extend_from_slice(&y.to_be_bytes());
        }
        self.write_record(RT_XY, DT_I32, &data)?;
        Ok(())
    }
}

pub fn f64_to_gds_bytes(value: f64) -> [u8; 8] {
    if value == 0.0 {
        return [0u8; 8];
    }

    let sign = value.is_sign_negative() as u8;
    let abs = value.abs();

    let log16 = abs.log10() / 16.0_f64.log10();
    let mut exponent = log16.ceil() as i32;
    exponent = exponent.clamp(0, 63);

    let base = 16.0_f64.powi(exponent);
    let mut mantissa_f = (abs / base) * (1u64 << 56) as f64;
    mantissa_f = mantissa_f.round();

    let mut mantissa = mantissa_f as u64;
    while mantissa >= (1u64 << 56) {
        mantissa /= 16;
        exponent += 1;
        if exponent > 63 {
            return if sign != 0 {
                [0x80, 0, 0, 0, 0, 0, 0, 0]
            } else {
                [0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
            };
        }
    }

    let sign_bit = (sign as u64) << 63;
    let exp_bits = ((exponent as u64 + 64) & 0x7F) << 56;
    let result = sign_bit | exp_bits | (mantissa & 0x00FFFFFFFFFFFFFF);
    result.to_be_bytes()
}

fn current_ymdhms() -> (u16, u16, u16, u16, u16, u16) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let total_days = secs / 86400;
    let remaining = secs % 86400;
    let hour = (remaining / 3600) as u16;
    let min = ((remaining % 3600) / 60) as u16;
    let sec = (remaining % 60) as u16;

    let mut year = 1970u16;
    let mut days_left = total_days;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days_left < days_in_year {
            break;
        }
        days_left -= days_in_year;
        year += 1;
    }

    let month_days: &[u16] = if is_leap_year(year) {
        &[0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335]
    } else {
        &[0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334]
    };

    let mut month = 1u16;
    for (i, &days) in month_days.iter().enumerate().skip(1) {
        if (days_left as u16) < days {
            month = i as u16;
            break;
        }
        if i == 11 {
            month = 12;
        }
    }

    let day = days_left as u16 - month_days[(month - 1) as usize] + 1;
    (year, month, day, hour, min, sec)
}

fn is_leap_year(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

pub fn sanitize_gds_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "CELL".to_string()
    } else {
        let result = trimmed.to_string();
        if result.len() > 32 {
            result[..32].to_string()
        } else {
            result
        }
    }
}

pub fn um_to_db(um: f64) -> i32 {
    (um / DB_UNIT_MICRONS).round() as i32
}

pub fn cell_structure_name(netlist: &Netlist, node_id: NodeId) -> String {
    let node = &netlist.nodes()[node_id.0];
    sanitize_gds_name(&format!("{}_{}", node.name, node_id.0))
}

pub fn node_kind_label(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::CellInstance => "CELL",
        NodeKind::MacroCell => "MACRO",
        NodeKind::Splitter => "SPLIT",
        NodeKind::Dff => "DFF",
        NodeKind::Jtl => "JTL",
        NodeKind::Ptl => "PTL",
        NodeKind::Port => "PORT",
    }
}

pub fn make_cell_boundary(x_um: f64, y_um: f64, w_um: f64, h_um: f64) -> GdsElement {
    let x0 = um_to_db(x_um);
    let y0 = um_to_db(y_um);
    let x1 = um_to_db(x_um + w_um);
    let y1 = um_to_db(y_um + h_um);
    GdsElement::Boundary(GdsBoundary {
        layer: LAYER_CELLS,
        datatype: DATATYPE_FILL,
        xy: vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)],
    })
}

pub fn make_cell_outline(x_um: f64, y_um: f64, w_um: f64, h_um: f64) -> GdsElement {
    let x0 = um_to_db(x_um);
    let y0 = um_to_db(y_um);
    let x1 = um_to_db(x_um + w_um);
    let y1 = um_to_db(y_um + h_um);
    GdsElement::Boundary(GdsBoundary {
        layer: LAYER_OUTLINE,
        datatype: DATATYPE_FILL,
        xy: vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)],
    })
}

pub fn make_port_marker(x_um: f64, y_um: f64) -> GdsElement {
    let cx = um_to_db(x_um);
    let cy = um_to_db(y_um);
    let r = um_to_db(PORT_RADIUS_UM);
    let points: Vec<(i32, i32)> = (0..=32)
        .map(|i| {
            let angle = (i as f64) * 2.0 * f64::consts::PI / 32.0;
            (
                cx + (r as f64 * angle.cos()) as i32,
                cy + (r as f64 * angle.sin()) as i32,
            )
        })
        .collect();
    GdsElement::Boundary(GdsBoundary {
        layer: LAYER_PORTS,
        datatype: DATATYPE_FILL,
        xy: points,
    })
}

pub fn make_label(x_um: f64, y_um: f64, label: &str) -> GdsElement {
    GdsElement::Text(GdsText {
        layer: LAYER_LABELS,
        texttype: 0,
        x: um_to_db(x_um),
        y: um_to_db(y_um + CELL_HEIGHT_UM + 2.0),
        string: label.to_string(),
    })
}

pub fn make_route_path(
    segments: &[(f64, f64, f64, f64, u8)],
) -> Option<GdsElement> {
    if segments.is_empty() {
        return None;
    }
    let mut xy = Vec::new();
    let first = &segments[0];
    xy.push((um_to_db(first.0), um_to_db(first.1)));
    for seg in segments {
        xy.push((um_to_db(seg.2), um_to_db(seg.3)));
    }
    let layer = segments[0].4 as u16;
    let width = um_to_db(PATH_WIDTH_UM);
    Some(GdsElement::Path(GdsPath {
        layer,
        datatype: DATATYPE_DEFAULT,
        width,
        xy,
    }))
}

pub fn write_gds<W: Write>(writer: W, library: &GdsLibrary) -> Result<(), IoError> {
    let mut gw = GdsWriter::new(writer);
    gw.write_library(library).map_err(IoError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f64_to_gds_bytes_zero() {
        let bytes = f64_to_gds_bytes(0.0);
        assert_eq!(bytes, [0u8; 8]);
    }

    #[test]
    fn f64_to_gds_bytes_positive_roundtrip() {
        let values = [1.0, 0.5, 100.0, 40.0, 24.0, 0.001, 1e-9];
        for &v in &values {
            let bytes = f64_to_gds_bytes(v);
            let restored = gds_bytes_to_f64(bytes);
            assert!(
                (v - restored).abs() / v.max(1e-30) < 1e-9,
                "roundtrip failed for {v}: got {restored}"
            );
        }
    }

    fn gds_bytes_to_f64(bytes: [u8; 8]) -> f64 {
        let bits = u64::from_be_bytes(bytes);
        let sign = (bits >> 63) & 1;
        let exponent = ((bits >> 56) & 0x7F) as i32 - 64;
        let mantissa = bits & 0x00FFFFFFFFFFFFFF;
        let value = (mantissa as f64) / ((1u64 << 56) as f64) * 16.0_f64.powi(exponent);
        if sign != 0 { -value } else { value }
    }

    #[test]
    fn sanitize_gds_name_replaces_special_chars() {
        assert_eq!(sanitize_gds_name("my-cell.name"), "my_cell_name");
        assert_eq!(sanitize_gds_name(""), "CELL");
        assert_eq!(sanitize_gds_name("___"), "CELL");
        assert_eq!(sanitize_gds_name("abc"), "abc");
    }

    #[test]
    fn um_to_db_converts_correctly() {
        assert_eq!(um_to_db(0.0), 0);
        assert_eq!(um_to_db(1.0), 1000);
        assert_eq!(um_to_db(40.0), 40000);
        assert_eq!(um_to_db(0.001), 1);
    }

    #[test]
    fn write_gds_produces_valid_binary() {
        let lib = GdsLibrary {
            name: "test_lib".to_string(),
            structures: vec![GdsStructure {
                name: "TOP".to_string(),
                elements: vec![GdsElement::Boundary(GdsBoundary {
                    layer: 0,
                    datatype: 0,
                    xy: vec![(0, 0), (1000, 0), (1000, 1000), (0, 1000), (0, 0)],
                })],
            }],
        };
        let mut buf = Vec::new();
        write_gds(&mut buf, &lib).expect("write should succeed");
        assert!(!buf.is_empty());
        assert_eq!(buf[0], 0x00);
        assert_eq!(buf[1], 0x06);
        assert_eq!(buf[2], RT_HEADER);
        assert_eq!(buf[3], DT_U16);
    }

    #[test]
    fn write_gds_with_path_element() {
        let lib = GdsLibrary {
            name: "test".to_string(),
            structures: vec![GdsStructure {
                name: "TOP".to_string(),
                elements: vec![GdsElement::Path(GdsPath {
                    layer: 1,
                    datatype: 0,
                    width: 2000,
                    xy: vec![(0, 0), (40000, 0), (40000, 24000)],
                })],
            }],
        };
        let mut buf = Vec::new();
        write_gds(&mut buf, &lib).expect("write should succeed");
        assert!(buf.len() > 100);
    }

    #[test]
    fn write_gds_with_sref_element() {
        let lib = GdsLibrary {
            name: "test".to_string(),
            structures: vec![
                GdsStructure {
                    name: "CELL_A".to_string(),
                    elements: vec![GdsElement::Boundary(GdsBoundary {
                        layer: 0,
                        datatype: 0,
                        xy: vec![(0, 0), (20000, 0), (20000, 12000), (0, 12000), (0, 0)],
                    })],
                },
                GdsStructure {
                    name: "TOP".to_string(),
                    elements: vec![GdsElement::Sref(GdsSref {
                        name: "CELL_A".to_string(),
                        x: 40000,
                        y: 24000,
                    })],
                },
            ],
        };
        let mut buf = Vec::new();
        write_gds(&mut buf, &lib).expect("write should succeed");
        assert!(buf.len() > 100);
    }

    #[test]
    fn make_route_path_returns_none_for_empty() {
        let result = make_route_path(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn make_route_path_returns_path_for_segments() {
        let segs = vec![(0.0, 0.0, 40.0, 0.0, 1u8), (40.0, 0.0, 40.0, 24.0, 1u8)];
        let result = make_route_path(&segs);
        assert!(result.is_some());
        match result.unwrap() {
            GdsElement::Path(p) => {
                assert_eq!(p.layer, 1);
                assert_eq!(p.xy.len(), 3);
            }
            _ => panic!("expected Path"),
        }
    }
}
