use std::io::{self, Write};

use rflux_ir::NodeKind;

use crate::IoError;

const MARGIN_UM: f64 = 20.0;
const CELL_WIDTH_UM: f64 = 20.0;
const CELL_HEIGHT_UM: f64 = 12.0;
const PORT_RADIUS_UM: f64 = 3.0;
const ROUTE_WIDTH_UM: f64 = 1.5;
const LABEL_FONT_SIZE: f64 = 4.0;
const TITLE_FONT_SIZE: f64 = 6.0;

struct Color {
    r: u8,
    g: u8,
    b: u8,
}

impl Color {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

const COLOR_CELL: Color = Color::new(173, 216, 230);
const COLOR_CELL_STROKE: Color = Color::new(0, 0, 139);
const COLOR_MACRO: Color = Color::new(255, 182, 193);
const COLOR_MACRO_STROKE: Color = Color::new(139, 0, 0);
const COLOR_DFF: Color = Color::new(144, 238, 144);
const COLOR_DFF_STROKE: Color = Color::new(0, 100, 0);
const COLOR_SPLITTER: Color = Color::new(255, 255, 200);
const COLOR_SPLITTER_STROKE: Color = Color::new(139, 139, 0);
const COLOR_PORT: Color = Color::new(255, 215, 0);
const COLOR_PORT_STROKE: Color = Color::new(139, 69, 19);
const COLOR_JTL: Color = Color::new(220, 50, 50);
const COLOR_PTL: Color = Color::new(50, 50, 220);
const COLOR_BACKGROUND: Color = Color::new(255, 255, 255);
const COLOR_GRID: Color = Color::new(230, 230, 230);

pub struct SvgCell {
    pub x_um: f64,
    pub y_um: f64,
    pub name: String,
    pub kind: NodeKind,
}

pub struct SvgRoute {
    pub points: Vec<(f64, f64)>,
    pub layer: u8,
}

pub struct SvgLayout {
    pub cells: Vec<SvgCell>,
    pub routes: Vec<SvgRoute>,
    pub width_um: f64,
    pub height_um: f64,
}

struct SvgRenderer<W: Write> {
    writer: W,
    width: f64,
    height: f64,
}

impl<W: Write> SvgRenderer<W> {
    fn new(writer: W, width: f64, height: f64) -> Self {
        Self { writer, width, height }
    }

    fn render(&mut self, layout: &SvgLayout, title: &str) -> io::Result<()> {
        self.write_header()?;
        self.write_background()?;
        self.write_grid(10.0)?;
        self.write_title(title)?;
        self.write_routes(layout)?;
        self.write_cells(layout)?;
        self.write_legend()?;
        self.write_footer()
    }

    fn write_header(&mut self) -> io::Result<()> {
        writeln!(
            self.writer,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg"
     viewBox="{} {} {} {}"
     width="{}mm" height="{}mm">"#,
            -MARGIN_UM,
            -MARGIN_UM,
            self.width + 2.0 * MARGIN_UM,
            self.height + 2.0 * MARGIN_UM,
            (self.width + 2.0 * MARGIN_UM) * 0.1,
            (self.height + 2.0 * MARGIN_UM) * 0.1,
        )
    }

    fn write_background(&mut self) -> io::Result<()> {
        writeln!(
            self.writer,
            r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
            -MARGIN_UM,
            -MARGIN_UM,
            self.width + 2.0 * MARGIN_UM,
            self.height + 2.0 * MARGIN_UM,
            COLOR_BACKGROUND.to_hex(),
        )
    }

    fn write_grid(&mut self, pitch: f64) -> io::Result<()> {
        let mut x = 0.0;
        while x <= self.width {
            writeln!(
                self.writer,
                r#"  <line x1="{}" y1="0" x2="{}" y2="{}" stroke="{}" stroke-width="0.1"/>"#,
                x, x, self.height, COLOR_GRID.to_hex(),
            )?;
            x += pitch;
        }
        let mut y = 0.0;
        while y <= self.height {
            writeln!(
                self.writer,
                r#"  <line x1="0" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="0.1"/>"#,
                y, self.width, y, COLOR_GRID.to_hex(),
            )?;
            y += pitch;
        }
        Ok(())
    }

    fn write_title(&mut self, text: &str) -> io::Result<()> {
        writeln!(
            self.writer,
            r#"  <text x="{}" y="{}" font-family="monospace" font-size="{}" text-anchor="middle" dominant-baseline="middle">{}</text>"#,
            self.width / 2.0,
            -MARGIN_UM / 2.0,
            TITLE_FONT_SIZE,
            escape_xml(text),
        )
    }

    fn write_routes(&mut self, layout: &SvgLayout) -> io::Result<()> {
        for route in &layout.routes {
            if route.points.len() < 2 {
                continue;
            }
            let color = if route.layer == 1 { &COLOR_JTL } else { &COLOR_PTL };
            write!(self.writer, r#"  <path d=""#)?;
            for (i, &(x, y)) in route.points.iter().enumerate() {
                if i == 0 {
                    write!(self.writer, "M {} {}", x, y)?;
                } else {
                    write!(self.writer, " L {} {}", x, y)?;
                }
            }
            writeln!(
                self.writer,
                r#"" fill="none" stroke="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"#,
                color.to_hex(),
                ROUTE_WIDTH_UM,
            )?;
        }
        Ok(())
    }

    fn write_cells(&mut self, layout: &SvgLayout) -> io::Result<()> {
        for cell in &layout.cells {
            let (fill, stroke) = node_colors(&cell.kind);
            writeln!(
                self.writer,
                r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="{}" stroke-width="0.3"/>"#,
                cell.x_um,
                cell.y_um,
                CELL_WIDTH_UM,
                CELL_HEIGHT_UM,
                fill.to_hex(),
                stroke.to_hex(),
            )?;

            if matches!(cell.kind, NodeKind::Port) {
                writeln!(
                    self.writer,
                    r#"  <circle cx="{}" cy="{}" r="{}" fill="{}" stroke="{}" stroke-width="0.2"/>"#,
                    cell.x_um + CELL_WIDTH_UM / 2.0,
                    cell.y_um + CELL_HEIGHT_UM / 2.0,
                    PORT_RADIUS_UM,
                    COLOR_PORT.to_hex(),
                    COLOR_PORT_STROKE.to_hex(),
                )?;
            }

            writeln!(
                self.writer,
                r#"  <text x="{}" y="{}" font-family="monospace" font-size="{}" text-anchor="middle" dominant-baseline="middle">{}</text>"#,
                cell.x_um + CELL_WIDTH_UM / 2.0,
                cell.y_um + CELL_HEIGHT_UM + LABEL_FONT_SIZE * 0.6,
                LABEL_FONT_SIZE,
                escape_xml(&cell.name),
            )?;
        }
        Ok(())
    }

    fn write_legend(&mut self) -> io::Result<()> {
        let x = self.width + 4.0;
        let mut y = 0.0;
        let items: &[(&str, &Color)] = &[
            ("Cell", &COLOR_CELL),
            ("Macro", &COLOR_MACRO),
            ("DFF", &COLOR_DFF),
            ("Split", &COLOR_SPLITTER),
            ("Port", &COLOR_PORT),
            ("JTL", &COLOR_JTL),
            ("PTL", &COLOR_PTL),
        ];

        for (label, color) in items {
            writeln!(
                self.writer,
                r#"  <rect x="{}" y="{}" width="6" height="4" fill="{}" stroke="{}" stroke-width="0.2"/>"#,
                x,
                y,
                color.to_hex(),
                COLOR_CELL_STROKE.to_hex(),
            )?;
            writeln!(
                self.writer,
                r#"  <text x="{}" y="{}" font-family="monospace" font-size="3" text-anchor="start" dominant-baseline="middle">{}</text>"#,
                x + 8.0,
                y + 2.0,
                label,
            )?;
            y += 6.0;
        }
        Ok(())
    }

    fn write_footer(&mut self) -> io::Result<()> {
        writeln!(self.writer, "</svg>")
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn node_colors(kind: &NodeKind) -> (&'static Color, &'static Color) {
    match kind {
        NodeKind::CellInstance => (&COLOR_CELL, &COLOR_CELL_STROKE),
        NodeKind::MacroCell => (&COLOR_MACRO, &COLOR_MACRO_STROKE),
        NodeKind::Splitter => (&COLOR_SPLITTER, &COLOR_SPLITTER_STROKE),
        NodeKind::Dff => (&COLOR_DFF, &COLOR_DFF_STROKE),
        NodeKind::Port => (&COLOR_PORT, &COLOR_PORT_STROKE),
        NodeKind::Jtl => (&COLOR_CELL, &COLOR_CELL_STROKE),
        NodeKind::Ptl => (&COLOR_CELL, &COLOR_CELL_STROKE),
    }
}

pub fn write_svg<W: Write>(writer: W, layout: &SvgLayout, title: &str) -> Result<(), IoError> {
    let mut renderer = SvgRenderer::new(writer, layout.width_um, layout.height_um);
    renderer.render(layout, title).map_err(IoError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_svg_produces_valid_output() {
        let layout = SvgLayout {
            cells: vec![
                SvgCell {
                    x_um: 0.0,
                    y_um: 0.0,
                    name: "a".to_string(),
                    kind: NodeKind::Port,
                },
                SvgCell {
                    x_um: 40.0,
                    y_um: 0.0,
                    name: "gate".to_string(),
                    kind: NodeKind::CellInstance,
                },
            ],
            routes: vec![SvgRoute {
                points: vec![(10.0, 6.0), (50.0, 6.0)],
                layer: 1,
            }],
            width_um: 60.0,
            height_um: 12.0,
        };
        let mut buf = Vec::new();
        write_svg(&mut buf, &layout, "test").expect("svg write should succeed");
        let svg = String::from_utf8(buf).expect("should be valid utf8");
        assert!(svg.contains("<?xml"));
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("a"));
        assert!(svg.contains("gate"));
    }

    #[test]
    fn write_svg_empty_layout() {
        let layout = SvgLayout {
            cells: Vec::new(),
            routes: Vec::new(),
            width_um: 0.0,
            height_um: 0.0,
        };
        let mut buf = Vec::new();
        write_svg(&mut buf, &layout, "empty").expect("svg write should succeed");
        let svg = String::from_utf8(buf).expect("should be valid utf8");
        assert!(svg.contains("empty"));
    }

    #[test]
    fn escape_xml_handles_special_chars() {
        assert_eq!(escape_xml("a<b>c"), "a&lt;b&gt;c");
        assert_eq!(escape_xml("a&b"), "a&amp;b");
        assert_eq!(escape_xml("\"x\""), "&quot;x&quot;");
    }
}
