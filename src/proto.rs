//! Wire format of the local Figma Agent API.
//!
//! Field names and casing were recovered from the macOS and Windows agents:
//! `fontFiles`, `variationAxes` are camelCase, while `modified_at`,
//! `user_installed` and `postscript` are snake_case/lowercase. Do not "fix"
//! the casing — Figma's client is strict about it.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    config::Settings,
    fonts::{FontFace, FontIndex, Instance},
};

#[derive(Debug, Clone, Serialize)]
pub struct VersionResponse {
    pub package: String,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct FontInfo {
    pub family: String,
    pub style: String,
    pub postscript: String,
    pub weight: u16,
    /// `usWidthClass`, 1..=9.
    pub stretch: u16,
    pub italic: bool,
    #[serde(rename = "variationAxes", skip_serializing_if = "Option::is_none")]
    pub variation_axes: Option<Vec<VariationAxisInfo>>,
    pub modified_at: u64,
    pub user_installed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VariationAxisInfo {
    pub tag: String,
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FontFilesResponse {
    #[serde(rename = "fontFiles")]
    pub font_files: BTreeMap<String, Vec<FontInfo>>,
    pub modified_at: Option<u64>,
    pub modified_fonts: Option<BTreeMap<String, Vec<FontInfo>>>,
    pub package: String,
    pub version: u32,
}

pub fn version_response(settings: &Settings) -> VersionResponse {
    VersionResponse {
        package: settings.package.clone(),
        version: settings.version,
    }
}

pub fn font_files_response(index: &FontIndex, settings: &Settings) -> FontFilesResponse {
    let font_files = index
        .files
        .iter()
        .map(|(path, file)| {
            let fonts = file
                .faces
                .iter()
                .flat_map(|face| face_payloads(face, file.modified_at))
                .collect();
            (path.clone(), fonts)
        })
        .collect();

    FontFilesResponse {
        font_files,
        // The official agent uses these for incremental updates of its cache.
        // A full list is cheap enough and both clients accept `null`.
        modified_at: None,
        modified_fonts: None,
        package: settings.package.clone(),
        version: settings.version,
    }
}

fn face_payloads(face: &FontFace, modified_at: u64) -> Vec<FontInfo> {
    let base = base_payload(face, modified_at, None);

    if face.instances.is_empty() {
        return vec![base];
    }

    face.instances
        .iter()
        .map(|instance| instance_payload(face, instance, modified_at))
        .collect()
}

fn base_payload(face: &FontFace, modified_at: u64, instance: Option<&Instance>) -> FontInfo {
    let variation_axes = if face.axes.is_empty() {
        None
    } else {
        Some(
            face.axes
                .iter()
                .enumerate()
                .map(|(index, axis)| {
                    let value = instance
                        .and_then(|instance| instance.coords.get(index).copied())
                        .unwrap_or(axis.value);
                    VariationAxisInfo {
                        tag: axis.tag.clone(),
                        name: axis.name.clone(),
                        value,
                        min: axis.min,
                        max: axis.max,
                        default: axis.default,
                        hidden: axis.hidden,
                    }
                })
                .collect(),
        )
    };

    FontInfo {
        family: face.family.clone(),
        style: face.style.clone(),
        postscript: face.postscript.clone(),
        weight: face.weight,
        stretch: face.stretch,
        italic: face.italic,
        variation_axes,
        modified_at,
        user_installed: true,
    }
}

/// A named instance is surfaced as a standalone font entry, with `wght`, `wdth`,
/// `ital` and `slnt` folded into the fields Figma understands.
fn instance_payload(face: &FontFace, instance: &Instance, modified_at: u64) -> FontInfo {
    let mut info = base_payload(face, modified_at, Some(instance));

    info.style = instance.name.clone();
    if let Some(postscript) = &instance.postscript {
        info.postscript = postscript.clone();
    }

    let mut italic = face.italic;
    let mut oblique = false;

    for (index, axis) in face.axes.iter().enumerate() {
        let Some(value) = instance.coords.get(index).copied() else {
            continue;
        };

        match axis.tag.as_str() {
            "wght" => info.weight = value.round().clamp(0.0, 1000.0) as u16,
            "wdth" => info.stretch = width_class(value),
            "ital" => italic = value >= 0.5,
            "slnt" => oblique = value.abs() > f32::EPSILON,
            _ => {}
        }
    }

    info.italic = italic || oblique;
    info
}

/// Maps a CSS-style width percentage onto the `usWidthClass` scale.
fn width_class(percentage: f32) -> u16 {
    const CLASSES: [f32; 9] = [50.0, 62.5, 75.0, 87.5, 100.0, 112.5, 125.0, 150.0, 200.0];

    let mut best = 5u16;
    let mut best_distance = f32::MAX;

    for (index, candidate) in CLASSES.iter().enumerate() {
        let distance = (percentage - candidate).abs();
        if distance < best_distance {
            best_distance = distance;
            best = index as u16 + 1;
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::width_class;

    #[test]
    fn width_percentages_map_to_classes() {
        assert_eq!(width_class(100.0), 5);
        assert_eq!(width_class(75.0), 3);
        assert_eq!(width_class(50.0), 1);
        assert_eq!(width_class(200.0), 9);
        assert_eq!(width_class(105.0), 5);
    }
}
