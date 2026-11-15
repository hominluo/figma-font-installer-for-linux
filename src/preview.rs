//! Font previews as SVG.
//!
//! Figma asks for a raster-ish preview of the font name when it builds its font
//! picker. The official agent embeds HarfBuzz and rasterizes; we emit an SVG
//! path built from the glyph outlines, which the browser renders for free.
//!
//! Limitations, compared to the official agent:
//! * no shaping (kerning/ligatures are ignored) — irrelevant for a font name;
//! * advance widths come from the base font, so `HVAR` width deltas of variable
//!   fonts are not applied. Outlines *do* honour the named instance.

use std::fmt::Write;

use ttf_parser::{Face, OutlineBuilder, Tag};

use crate::fonts::Axis;

pub fn render_svg(
    data: &[u8],
    face_index: u32,
    text: &str,
    size_px: f32,
    axes: &[Axis],
    coords: &[f32],
) -> Option<String> {
    // Family names are short; capping keeps a crafted request from turning
    // into a very large SVG.
    let text: String = text.trim().chars().take(128).collect();
    if text.is_empty() {
        return None;
    }

    let mut face = Face::parse(data, face_index).ok()?;

    for (index, axis) in axes.iter().enumerate() {
        let Some(value) = coords.get(index).copied() else {
            continue;
        };
        if let Ok(bytes) = <[u8; 4]>::try_from(axis.tag.as_bytes()) {
            let _ = face.set_variation(Tag::from_bytes(&bytes), value);
        }
    }

    let units_per_em = face.units_per_em().max(1) as f32;
    let size = size_px.clamp(4.0, 4096.0);
    let scale = size / units_per_em;

    let mut ascent = face.ascender() as f32 * scale;
    let mut descent = face.descender() as f32 * scale;
    if !ascent.is_finite() || ascent <= 0.0 {
        ascent = size * 0.8;
    }
    if !descent.is_finite() || descent >= 0.0 {
        descent = -size * 0.2;
    }

    let padding = (size * 0.15).max(2.0);
    let baseline = padding + ascent;
    let mut pen = padding;
    let mut path = String::new();

    for character in text.chars() {
        let glyph = face.glyph_index(character)?;
        // `ttf-parser` returns glyph 0 (`.notdef`) when the cmap has no entry.
        if glyph.0 == 0 && character != ' ' {
            return None;
        }

        let mut builder = SvgPath::new(pen, baseline, scale);
        face.outline_glyph(glyph, &mut builder);
        path.push_str(&builder.into_data());

        let advance = face.glyph_hor_advance(glyph).unwrap_or(0) as f32 * scale;
        pen += advance;
    }

    if path.is_empty() {
        return None;
    }

    let width = (pen + padding).ceil().max(1.0);
    let height = (ascent - descent + padding * 2.0).ceil().max(1.0);

    let mut svg = String::with_capacity(path.len() + 256);
    let _ = write!(
        svg,
        r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.0} {height:.0}"><path fill="#000000" d="{}"/></svg>"##,
        path.trim_end()
    );

    Some(svg)
}

struct SvgPath {
    data: String,
    origin_x: f32,
    origin_y: f32,
    scale: f32,
}

impl SvgPath {
    fn new(origin_x: f32, origin_y: f32, scale: f32) -> Self {
        Self {
            data: String::new(),
            origin_x,
            origin_y,
            scale,
        }
    }

    fn into_data(self) -> String {
        self.data
    }

    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.origin_x + x * self.scale,
            self.origin_y - y * self.scale,
        )
    }
}

impl OutlineBuilder for SvgPath {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        let _ = write!(self.data, "M{x:.2} {y:.2}");
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        let _ = write!(self.data, "L{x:.2} {y:.2}");
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x, y) = self.point(x, y);
        let _ = write!(self.data, "Q{x1:.2} {y1:.2} {x:.2} {y:.2}");
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.point(x1, y1);
        let (x2, y2) = self.point(x2, y2);
        let (x, y) = self.point(x, y);
        let _ = write!(self.data, "C{x1:.2} {y1:.2} {x2:.2} {y2:.2} {x:.2} {y:.2}");
    }

    fn close(&mut self) {
        self.data.push('Z');
    }
}
