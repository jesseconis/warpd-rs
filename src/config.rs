use serde::{Deserialize, Serialize};
use std::path::Path;
use std::path::PathBuf;

/// All configurable options for warpd-rs, matching the original warpd config keys.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    // -- Hint mode visuals --
    /// Source of hint targets: "grid", "stdin", or "detect".
    pub hint_source: String,
    /// Characters used to generate hint labels (order matters).
    pub hint_chars: String,
    /// Background colour of hint boxes (CSS hex).
    pub hint_bgcolor: String,
    /// Opacity multiplier for hint background fill (0.0..1.0).
    pub hint_bg_opacity: f64,
    /// Foreground / text colour of hint boxes.
    pub hint_fgcolor: String,

    /// Border radius in pixels for hint rounded-rect.
    pub hint_border_radius: f64,
    /// Font family used to render hint labels.
    pub hint_font: String,
    /// Font size in pixels for hint labels.
    pub hint_font_size: f64,
    /// Spacing between hint grid cells in pixels.
    pub hint_grid_gap: u32,

    // -- Grid mode --
    pub grid_color: String,
    pub grid_border_size: u32,
    /// Font size in pixels for the grid quadrant labels.
    pub grid_font_size: f64,
    /// Minimum width/height (in pixels) before grid auto-selects centre.
    pub grid_min_size: f64,
    /// Quadrant selection keys for grid mode (TL, TR, BL, BR order).
    pub grid_quadrant_keys: [String; 4],

    // -- Normal mode --
    pub speed: u32,
    pub acceleration: u32,
    /// Normal mode movement keys (left, down, up, right order).
    pub normal_move_keys: [String; 4],
    /// Key binding that boosts cursor speed when held.
    pub speed_modifier_key: String,
    /// Multiplier applied when the speed modifier is active.
    pub speed_modifier_multiplier: f64,

    // -- normal mode appearance
    pub cursor_color: String,
    pub cursor_size: u32,
    pub crosshair_line_width: u32,

    // -- Mouse buttons (warpd convention: left, middle, right) --
    pub buttons: [String; 3],
    /// If true, hint/grid auto-warp also emits an implicit left click.
    pub click_after_warp: bool,
}

#[cfg_attr(not(feature = "gui"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    Disk,
    Defaults,
}

#[cfg_attr(not(feature = "gui"), allow(dead_code))]
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: Config,
    pub path: PathBuf,
    pub source: ConfigSource,
    pub load_error: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hint_source: "static".into(),
            hint_chars: "abcdefghijklmnopqrstuvwxyz".into(),
            hint_bgcolor: "#1e1e2e".into(),
            hint_bg_opacity: 1.0,
            hint_fgcolor: "#cdd6f4".into(),
            hint_border_radius: 4.0,
            hint_font: "monospace".into(),
            hint_font_size: 16.0,
            hint_grid_gap: 80,

            grid_color: "#89b4fa".into(),
            grid_border_size: 2,
            grid_font_size: 36.0,
            grid_min_size: 270.0,
            grid_quadrant_keys: ["u".into(), "i".into(), "j".into(), "k".into()],

            speed: 220,
            acceleration: 700,
            normal_move_keys: ["h".into(), "j".into(), "k".into(), "l".into()],
            speed_modifier_key: "SHIFT".into(),
            speed_modifier_multiplier: 5.0,

            buttons: ["m".into(), ",".into(), ".".into()],
            click_after_warp: true,

            crosshair_line_width: 2,
            cursor_color: "#f38ba8".into(),
            cursor_size: 7,
        }
    }
}

impl Config {
    pub fn candidate_paths() -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(xdg) = dirs::config_dir() {
            candidates.push(xdg.join("warpd-rs").join("config.toml"));
        }

        if let Some(home) = dirs::home_dir() {
            let home_cfg = home.join(".config/warpd-rs/config.toml");
            if !candidates.contains(&home_cfg) {
                candidates.push(home_cfg);
            }
        }

        candidates
    }

    pub fn preferred_path() -> PathBuf {
        Self::candidate_paths()
            .into_iter()
            .next()
            .unwrap_or_else(|| PathBuf::from("config.toml"))
    }

    pub fn load_with_source(path: Option<&Path>) -> LoadedConfig {
        if let Some(path) = path {
            match std::fs::read_to_string(path) {
                Ok(text) => match toml::from_str::<Config>(&text) {
                    Ok(cfg) => {
                        log::info!("loaded config from {}", path.display());
                        log::debug!("{:#?}", cfg);
                        return LoadedConfig {
                            config: cfg,
                            path: path.to_path_buf(),
                            source: ConfigSource::Disk,
                            load_error: None,
                        };
                    }
                    Err(error) => {
                        let message = format!("bad config at {}: {error}", path.display());
                        log::warn!("{message}");
                        log::info!("using defaults after explicit config load failure");
                        let config = Config::default();
                        log::debug!("{:#?}", config);
                        return LoadedConfig {
                            config,
                            path: path.to_path_buf(),
                            source: ConfigSource::Defaults,
                            load_error: Some(message),
                        };
                    }
                },
                Err(error) => {
                    let message = format!("cannot read {}: {error}", path.display());
                    log::warn!("{message}");
                    log::info!("using defaults after explicit config load failure");
                    let config = Config::default();
                    log::debug!("{:#?}", config);
                    return LoadedConfig {
                        config,
                        path: path.to_path_buf(),
                        source: ConfigSource::Defaults,
                        load_error: Some(message),
                    };
                }
            }
        }

        let candidates = Self::candidate_paths();
        let mut first_error: Option<(PathBuf, String)> = None;

        for candidate in &candidates {
            if !candidate.exists() {
                continue;
            }

            match std::fs::read_to_string(candidate) {
                Ok(text) => match toml::from_str::<Config>(&text) {
                    Ok(cfg) => {
                        log::info!("loaded config from {}", candidate.display());
                        log::debug!("{:#?}", cfg);
                        return LoadedConfig {
                            config: cfg,
                            path: candidate.clone(),
                            source: ConfigSource::Disk,
                            load_error: None,
                        };
                    }
                    Err(error) => {
                        let message = format!("bad config at {}: {error}", candidate.display());
                        log::warn!("{message}");
                        if first_error.is_none() {
                            first_error = Some((candidate.clone(), message));
                        }
                    }
                },
                Err(error) => {
                    let message = format!("cannot read {}: {error}", candidate.display());
                    log::warn!("{message}");
                    if first_error.is_none() {
                        first_error = Some((candidate.clone(), message));
                    }
                }
            }
        }

        let config = Config::default();
        match first_error {
            Some((path, message)) => {
                log::info!("using defaults after config load failure");
                log::debug!("{:#?}", config);
                LoadedConfig {
                    config,
                    path,
                    source: ConfigSource::Defaults,
                    load_error: Some(message),
                }
            }
            None => {
                log::info!("no config file found, using defaults");
                log::debug!("{:#?}", config);
                LoadedConfig {
                    config,
                    path: Self::preferred_path(),
                    source: ConfigSource::Defaults,
                    load_error: None,
                }
            }
        }
    }

    /// Load configuration, falling back to defaults for any missing keys.
    /// If `path` is provided, only that file is considered.
    /// Otherwise search order is: $XDG_CONFIG_HOME/warpd-rs/config.toml  →  ~/.config/warpd-rs/config.toml
    pub fn load(path: Option<&Path>) -> Self {
        Self::load_with_source(path).config
    }

    pub fn to_pretty_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let toml = self
            .to_pretty_toml()
            .expect("failed to serialize config to TOML");
        std::fs::write(path, toml)
    }

    pub fn create_config(path: &Path) -> std::io::Result<()> {
        Config::default().save_to_path(path)
    }
}

#[cfg(feature = "gui")]
pub fn is_valid_hex_color(hex: &str) -> bool {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    matches!(hex.len(), 6 | 8) && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

pub fn color32_from_hex(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    let (r, g, b, a) = match hex.len() {
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            255,
        ),
        8 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            u8::from_str_radix(&hex[6..8], 16).ok()?,
        ),
        _ => return None,
    };

    Some([r, g, b, a])
}

#[cfg(feature = "gui")]
pub fn hex_from_rgba8(rgba: [u8; 4]) -> String {
    if rgba[3] == 255 {
        format!("#{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2])
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            rgba[0], rgba[1], rgba[2], rgba[3]
        )
    }
}

/// Parse a CSS hex colour string (#RRGGBB or #RRGGBBAA) into (r, g, b, a) with each in 0.0..1.0.
pub fn parse_hex_color(hex: &str) -> (f64, f64, f64, f64) {
    let (r, g, b, a) = color32_from_hex(hex).map_or((0, 0, 0, 255), |[r, g, b, a]| (r, g, b, a));
    (
        r as f64 / 255.0,
        g as f64 / 255.0,
        b as f64 / 255.0,
        a as f64 / 255.0,
    )
}
