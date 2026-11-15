//! Font discovery and parsing.
//!
//! Discovery uses `fc-list` (fontconfig) so that everything the user sees in
//! their desktop font picker is also visible to Figma. If fontconfig is not
//! available we fall back to walking the standard font directories. The index is
//! rebuilt incrementally: files whose mtime and size did not change keep their
//! parsed faces.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use ttf_parser::{name_id, Face, Tag};

use crate::config::Settings;

#[derive(Debug, Clone, Default)]
pub struct FontIndex {
    /// Keyed by the absolute path as reported by fontconfig. The key is exactly
    /// what Figma sends back in `?file=`, so it must not be normalized.
    pub files: BTreeMap<String, FontFile>,
    pub face_count: usize,
}

#[derive(Debug, Clone)]
pub struct FontFile {
    pub path: PathBuf,
    pub modified_at: u64,
    pub size: u64,
    pub faces: Vec<FontFace>,
}

#[derive(Debug, Clone)]
pub struct FontFace {
    /// Face index inside the file (non-zero for TrueType/OpenType collections).
    pub index: u32,
    pub family: String,
    pub style: String,
    pub postscript: String,
    pub weight: u16,
    pub stretch: u16,
    pub italic: bool,
    pub axes: Vec<Axis>,
    /// Named instances of a variable font (`fvar` instance records). When a
    /// font has them, each instance is advertised as its own font, mirroring
    /// the official agent.
    pub instances: Vec<Instance>,
}

#[derive(Debug, Clone)]
pub struct Axis {
    pub tag: String,
    pub name: String,
    pub min: f32,
    pub value: f32,
    pub max: f32,
    pub default: f32,
    pub hidden: bool,
}

#[derive(Debug, Clone)]
pub struct Instance {
    pub name: String,
    pub postscript: Option<String>,
    /// One user-space value per axis, in `fvar` order.
    pub coords: Vec<f32>,
}

impl FontFile {
    /// Resolves the face (and optional named instance) a preview request refers
    /// to. Figma identifies fonts primarily by PostScript name and falls back to
    /// family + style.
    pub fn find(
        &self,
        family: &str,
        style: &str,
        postscript: &str,
    ) -> Option<(&FontFace, Option<&Instance>)> {
        if !postscript.is_empty() {
            for face in &self.faces {
                if face.postscript == postscript {
                    return Some((face, None));
                }
            }
            for face in &self.faces {
                if let Some(instance) = face
                    .instances
                    .iter()
                    .find(|instance| instance.postscript.as_deref() == Some(postscript))
                {
                    return Some((face, Some(instance)));
                }
            }
        }

        if !family.is_empty() || !style.is_empty() {
            for face in &self.faces {
                let family_matches = family.is_empty() || face.family == family;
                if family_matches && (style.is_empty() || face.style == style) {
                    return Some((face, None));
                }
            }
            for face in &self.faces {
                if family.is_empty() || face.family == family {
                    if let Some(instance) = face.instances.iter().find(|i| i.name == style) {
                        return Some((face, Some(instance)));
                    }
                }
            }
        }

        if family.is_empty() && style.is_empty() && postscript.is_empty() {
            return self.faces.first().map(|face| (face, None));
        }

        None
    }
}

/// Rebuilds the index. `previous` enables incremental parsing.
pub fn scan(settings: &Settings, previous: Option<&FontIndex>) -> Result<FontIndex> {
    let started = SystemTime::now();
    let paths = discover_font_files(settings);

    let mut files = BTreeMap::new();
    let mut face_count = 0usize;
    let mut parsed = 0usize;
    let mut reused = 0usize;
    let mut failed = 0usize;

    for path in paths {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };

        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let size = metadata.len();
        let key = path.to_string_lossy().into_owned();

        if let Some(cached) = previous.and_then(|index| index.files.get(&key)) {
            if cached.modified_at == modified_at && cached.size == size && !cached.faces.is_empty()
            {
                face_count += cached.faces.len();
                files.insert(key, cached.clone());
                reused += 1;
                continue;
            }
        }

        match parse_font_file(&path) {
            Ok(Some(faces)) => {
                face_count += faces.len();
                parsed += 1;
                files.insert(
                    key,
                    FontFile {
                        path,
                        modified_at,
                        size,
                        faces,
                    },
                );
            }
            Ok(None) => failed += 1,
            Err(error) => {
                failed += 1;
                debug!("skipping {}: {error:#}", path.display());
            }
        }
    }

    let summary = format!(
        "font index: {} faces in {} files ({} parsed, {} reused, {} unusable) in {} ms",
        face_count,
        files.len(),
        parsed,
        reused,
        failed,
        started.elapsed().map(|d| d.as_millis()).unwrap_or(0)
    );

    // A quiet periodic rescan should not fill the journal every five minutes.
    if previous.is_none() || parsed > 0 {
        info!("{summary}");
    } else {
        debug!("{summary}");
    }

    Ok(FontIndex { files, face_count })
}

/// Every font file fontconfig knows about, plus anything in the user's font
/// directories and the extra directories from the config file.
///
/// The user directories are walked even when `fc-list` works: dropping a font
/// into `~/.local/share/fonts` should show up without remembering to run
/// `fc-cache` first. Duplicates are deduplicated by path.
pub fn discover_font_files(settings: &Settings) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    let mut used_fontconfig = false;

    if let Ok(output) = Command::new("fc-list")
        .args(["--format=%{file}\n"])
        .output()
    {
        if output.status.success() {
            used_fontconfig = true;
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let path = PathBuf::from(line.trim());
                if is_supported(&path) {
                    paths.insert(path);
                }
            }
        } else {
            debug!("fc-list exited with {}", output.status);
        }
    } else {
        debug!("fc-list not found, walking standard font directories");
    }

    if !used_fontconfig || paths.is_empty() {
        for directory in system_font_directories() {
            walk(&directory, &mut paths, 0);
        }
    }

    for directory in user_font_directories() {
        walk(&directory, &mut paths, 0);
    }

    for directory in &settings.font_directories {
        walk(directory, &mut paths, 0);
    }

    paths.into_iter().filter(|path| path.is_file()).collect()
}

fn system_font_directories() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
        // Flatpak and toolbox expose the host's fonts here.
        PathBuf::from("/run/host/fonts"),
    ]
}

fn user_font_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();

    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME").filter(|dir| !dir.is_empty()) {
        directories.push(PathBuf::from(data_home).join("fonts"));
    }

    if let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) {
        directories.push(PathBuf::from(&home).join(".local/share/fonts"));
        directories.push(PathBuf::from(&home).join(".fonts"));
    }

    directories
}

fn walk(directory: &Path, out: &mut BTreeSet<PathBuf>, depth: usize) {
    if depth > 8 {
        return;
    }

    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        // `metadata()` follows symlinks, which font directories rely on.
        let path = entry.path();
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };

        if metadata.is_dir() {
            walk(&path, out, depth + 1);
        } else if metadata.is_file() && is_supported(&path) {
            out.insert(path);
        }
    }
}

fn is_supported(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    matches!(extension.as_str(), "ttf" | "otf" | "ttc" | "otc")
}

fn parse_font_file(path: &Path) -> Result<Option<Vec<FontFace>>> {
    let data = fs::read(path).with_context(|| format!("could not read {}", path.display()))?;

    let count = ttf_parser::fonts_in_collection(&data)
        .unwrap_or(1)
        .clamp(1, 64);

    let mut faces = Vec::new();
    for index in 0..count {
        if let Some(face) = parse_face(&data, index) {
            faces.push(face);
        }
    }

    Ok(if faces.is_empty() { None } else { Some(faces) })
}

fn parse_face(data: &[u8], index: u32) -> Option<FontFace> {
    let face = Face::parse(data, index).ok()?;
    let names = face.names();

    let family = name_from_ids(
        &names,
        &[
            name_id::TYPOGRAPHIC_FAMILY,
            name_id::FAMILY,
            name_id::FULL_NAME,
        ],
    )
    .unwrap_or_else(|| format!("Unknown {index}"));
    let style = name_from_ids(
        &names,
        &[name_id::TYPOGRAPHIC_SUBFAMILY, name_id::SUBFAMILY],
    )
    .unwrap_or_else(|| "Regular".to_string());

    let postscript = name_from_ids(&names, &[name_id::POST_SCRIPT_NAME])
        .unwrap_or_else(|| format!("{}-{}", compact(&family), compact(&style)));

    let os2 = face.tables().os2;
    let weight = os2
        .map(|table| table.weight().to_number())
        .unwrap_or_else(|| face.weight().to_number());
    let stretch = os2
        .map(|table| table.width().to_number())
        .unwrap_or_else(|| face.width().to_number());

    let axes: Vec<Axis> = face
        .variation_axes()
        .into_iter()
        .map(|axis| Axis {
            tag: axis.tag.to_string(),
            name: standardized_name(&names, axis.name_id),
            min: axis.min_value,
            value: axis.def_value,
            max: axis.max_value,
            default: axis.def_value,
            hidden: axis.hidden,
        })
        .collect();

    let instances = if axes.is_empty() {
        Vec::new()
    } else {
        parse_named_instances(&face, &names, &axes, &family)
    };

    Some(FontFace {
        index,
        family,
        style,
        postscript,
        weight,
        stretch,
        italic: face.is_italic() || face.is_oblique(),
        axes,
        instances,
    })
}

/// Resolves the first name available for the given name IDs. Prefers records
/// with a more "canonical" language: Windows English (0x0409) first, then any
/// Windows/Unicode record.
fn name_from_ids(names: &ttf_parser::name::Names, ids: &[u16]) -> Option<String> {
    for id in ids {
        let mut fallback: Option<String> = None;

        for name in *names {
            if name.name_id != *id || !name.is_unicode() {
                continue;
            }

            let Some(value) = name.to_string() else {
                continue;
            };
            let value = value.trim().to_string();
            if value.is_empty() {
                continue;
            }

            if name.language_id == 0x0409 {
                return Some(value);
            }
            if fallback.is_none() {
                fallback = Some(value);
            }
        }

        if fallback.is_some() {
            return fallback;
        }
    }

    None
}

fn standardized_name(names: &ttf_parser::name::Names, id: u16) -> String {
    name_from_ids(names, &[id]).unwrap_or_default()
}

fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// Parses the `fvar` instance records that `ttf-parser` does not expose.
///
/// Layout (OpenType spec, `fvar`):
/// ```text
/// 0  u16 major
/// 2  u16 minor
/// 4  u16 axesArrayOffset
/// 6  u16 reserved
/// 8  u16 axisCount
/// 10 u16 axisSize          (20)
/// 12 u16 instanceCount
/// 14 u16 instanceSize
/// ... axes records ...
/// ... instance records: u16 subfamilyNameID, u16 flags, Fixed coords[axisCount],
///     optional u16 postScriptNameID when instanceSize is large enough ...
/// ```
fn parse_named_instances(
    face: &Face,
    names: &ttf_parser::name::Names,
    axes: &[Axis],
    family: &str,
) -> Vec<Instance> {
    let Some(data) = face.raw_face().table(Tag::from_bytes(b"fvar")) else {
        return Vec::new();
    };

    let axis_count = match be_u16(data, 8) {
        Some(count) => usize::from(count),
        None => return Vec::new(),
    };
    let axis_size = usize::from(be_u16(data, 10).unwrap_or(20));
    let instance_count = usize::from(be_u16(data, 12).unwrap_or(0));
    let instance_size = usize::from(be_u16(data, 14).unwrap_or(0));
    let axes_offset = usize::from(be_u16(data, 4).unwrap_or(16));

    if axis_count == 0
        || axis_count != axes.len()
        || axis_size < 20
        || instance_size < 4 + axis_count * 4
        || instance_count == 0
    {
        return Vec::new();
    }

    let instances_offset = axes_offset + axis_count * axis_size;
    let has_postscript_name = instance_size >= 4 + axis_count * 4 + 2;

    let mut instances = Vec::with_capacity(instance_count);

    for index in 0..instance_count {
        let base = instances_offset + index * instance_size;

        let Some(subfamily_name_id) = be_u16(data, base) else {
            break;
        };

        let mut coords = Vec::with_capacity(axis_count);
        for axis in 0..axis_count {
            let Some(value) = be_u32(data, base + 4 + axis * 4) else {
                return instances;
            };
            coords.push(fixed_to_f32(value));
        }

        let postscript = if has_postscript_name {
            be_u16(data, base + 4 + axis_count * 4).and_then(|id| standardized_name_opt(names, id))
        } else {
            None
        };

        let name = match standardized_name_opt(names, subfamily_name_id) {
            Some(name) => name,
            None => continue,
        };

        let postscript =
            postscript.or_else(|| Some(format!("{}-{}", compact(family), compact(&name))));

        instances.push(Instance {
            name,
            postscript,
            coords,
        });
    }

    instances
}

fn standardized_name_opt(names: &ttf_parser::name::Names, id: u16) -> Option<String> {
    if id == 0 || id == 0xFFFF {
        return None;
    }
    let name = name_from_ids(names, &[id])?;
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn be_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset + 2)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn fixed_to_f32(value: u32) -> f32 {
    (value as i32) as f32 / 65536.0
}
