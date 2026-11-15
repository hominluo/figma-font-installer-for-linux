//! Configuration: optional JSON file plus CLI overrides.
//!
//! The defaults are the values the official Figma Agent reports. Figma only
//! checks that `package`/`version` are present and recent enough, so they are
//! configurable for future clients.

use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// `CFBundleShortVersionString` of the macOS agent this protocol was recovered
/// from.
pub const DEFAULT_PACKAGE: &str = "126.11.9";
/// The numeric `version` field that came with that release.
pub const DEFAULT_VERSION: u32 = 24;

/// The port the web app probes first.
pub const DEFAULT_PORT: u16 = 44950;
/// The official agent also listens here; kept as a fallback.
pub const FALLBACK_PORT: u16 = 44960;

#[derive(Debug, Default, Deserialize)]
pub struct ConfigFile {
    pub package: Option<String>,
    pub version: Option<u32>,
    pub ports: Option<Vec<u16>>,
    /// Extra directories to scan, on top of what fontconfig reports.
    pub font_directories: Option<Vec<PathBuf>>,
    /// How often to look for new or changed font files. `0` disables rescans.
    pub rescan_interval_secs: Option<u64>,
    /// Render font previews as SVG. Disabling makes `/figma/font-preview` 404.
    pub enable_preview: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub package: String,
    pub version: u32,
    pub ports: Vec<u16>,
    pub font_directories: Vec<PathBuf>,
    pub rescan_interval: Duration,
    pub enable_preview: bool,
    pub config_path: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            package: DEFAULT_PACKAGE.to_string(),
            version: DEFAULT_VERSION,
            ports: vec![DEFAULT_PORT, FALLBACK_PORT],
            font_directories: Vec::new(),
            rescan_interval: Duration::from_secs(300),
            enable_preview: true,
            config_path: None,
        }
    }
}

/// `$XDG_CONFIG_HOME/figma-font-agent/config.json`, falling back to
/// `$HOME/.config/figma-font-agent/config.json`.
pub fn default_config_path() -> Option<PathBuf> {
    if let Some(dir) = env::var_os("XDG_CONFIG_HOME").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir).join("figma-font-agent/config.json"));
    }

    env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(".config/figma-font-agent/config.json"))
}

/// Loads the configuration. A missing *implicit* config file is fine; a missing
/// file that was passed with `--config` is an error, so typos are noticed.
pub fn load(explicit: Option<&Path>) -> Result<Settings> {
    let mut settings = Settings::default();

    let path = match explicit {
        Some(path) => Some(path.to_path_buf()),
        None => default_config_path().filter(|path| path.is_file()),
    };

    let Some(path) = path else {
        return Ok(settings);
    };

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read config file {}", path.display()))?;

    let parsed: ConfigFile = serde_json::from_str(&raw)
        .with_context(|| format!("could not parse config file {}", path.display()))?;

    if let Some(package) = parsed.package {
        settings.package = package;
    }
    if let Some(version) = parsed.version {
        settings.version = version;
    }
    if let Some(ports) = parsed.ports {
        if ports.is_empty() {
            bail!("config `ports` must contain at least one port");
        }
        settings.ports = ports;
    }
    if let Some(directories) = parsed.font_directories {
        settings.font_directories = directories;
    }
    if let Some(interval) = parsed.rescan_interval_secs {
        settings.rescan_interval = Duration::from_secs(interval);
    }
    if let Some(enable) = parsed.enable_preview {
        settings.enable_preview = enable;
    }

    settings.config_path = Some(path);
    Ok(settings)
}

/// Writes a commented example config next to the default location, unless one
/// already exists. Used by `--init-config`.
pub fn write_example(path: &Path) -> Result<()> {
    if path.exists() {
        bail!(
            "{} already exists; not overwriting it (edit it directly, or remove it first)",
            path.display()
        );
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }

    let example = r#"{
  "package": "126.11.9",
  "version": 24,
  "ports": [44950, 44960],
  "font_directories": [],
  "rescan_interval_secs": 300,
  "enable_preview": true
}
"#;

    std::fs::write(path, example).with_context(|| format!("could not write {}", path.display()))
}
