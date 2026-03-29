use serde::{Deserialize, Serialize};
use std::path::Path;
use std::path::PathBuf;

/// All configurable options for warpd-rs, matching the original warpd config keys.
#[derive(Debug, Clone, Deserialize, Serialize)]
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

            crosshair_line_width: 2,
            cursor_color: "#f38ba8".into(),
            cursor_size: 7,
        }
    }
}

impl Config {
    /// Load configuration, falling back to defaults for any missing keys.
    /// If `path` is provided, only that file is considered.
    /// Otherwise search order is: $XDG_CONFIG_HOME/warpd-rs/config.toml  →  ~/.config/warpd-rs/config.toml
    pub fn load(path: Option<&Path>) -> Self {
        if let Some(path) = path {
            match std::fs::read_to_string(path) {
                Ok(text) => match toml::from_str::<Config>(&text) {
                    Ok(cfg) => {
                        log::info!("loaded config from {}", path.display());
                        log::debug!("{:#?}", cfg);
                        return cfg;
                    }
                    Err(e) => {
                        log::warn!("bad config at {}: {e}", path.display());
                    }
                },
                Err(e) => {
                    log::warn!("cannot read {}: {e}", path.display());
                }
            }

            log::info!("using defaults after explicit config load failure");
            log::debug!("{:#?}", Self::default());
            return Config::default();
        }

        let candidates: Vec<PathBuf> = {
            let mut v = Vec::new();
            if let Some(xdg) = dirs::config_dir() {
                v.push(xdg.join("warpd-rs").join("config.toml"));
            }
            let home_cfg = dirs::home_dir().map(|h| h.join(".config/warpd-rs/config.toml"));
            if let Some(p) = home_cfg {
                if !v.contains(&p) {
                    v.push(p);
                }
            }
            v
        };

        for path in &candidates {
            if path.exists() {
                match std::fs::read_to_string(path) {
                    Ok(text) => match toml::from_str::<Config>(&text) {
                        Ok(cfg) => {
                            log::info!("loaded config from {}", path.display());
                            log::debug!("{:#?}", cfg);
                            return cfg;
                        }
                        Err(e) => {
                            log::warn!("bad config at {}: {e}", path.display());
                        }
                    },
                    Err(e) => {
                        log::warn!("cannot read {}: {e}", path.display());
                    }
                }
            }
        }

        log::info!("no config file found, using defaults");
        log::debug!("{:#?}", Self::default());
        Config::default()
    }

    pub fn create_config(path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let default_cfg = Config::default();
        let toml_str =
            toml::to_string_pretty(&default_cfg).expect("failed to serialize default config");
        std::fs::write(path, toml_str)?;
        Ok(())
    }
}

/// Parse a CSS hex colour string (#RRGGBB or #RRGGBBAA) into (r, g, b, a) with each in 0.0..1.0.
pub fn parse_hex_color(hex: &str) -> (f64, f64, f64, f64) {
    let hex = hex.trim_start_matches('#');
    let (r, g, b, a) = match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            (r, g, b, 255u8)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            let a = u8::from_str_radix(&hex[6..8], 16).unwrap_or(255);
            (r, g, b, a)
        }
        _ => (0, 0, 0, 255),
    };
    (
        r as f64 / 255.0,
        g as f64 / 255.0,
        b as f64 / 255.0,
        a as f64 / 255.0,
    )
}
