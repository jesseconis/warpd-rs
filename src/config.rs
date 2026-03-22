use serde::Deserialize;
use std::path::Path;
use std::path::PathBuf;

/// All configurable options for warpd-rs, matching the original warpd config keys.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    // -- Hint mode visuals --
    /// Characters used to generate hint labels (order matters).
    pub hint_chars: String,
    /// Background colour of hint boxes (CSS hex).
    pub hint_bgcolor: String,
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
    /// Minimum width/height (in pixels) before grid auto-selects centre.
    pub grid_min_size: f64,

    // -- Normal mode movement --
    pub speed: u32,
    pub acceleration: u32,

    // -- Mouse buttons (warpd convention: left, middle, right) --
    pub buttons: [String; 3],

    // -- Colours / misc --
    pub cursor_color: String,
    pub cursor_size: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hint_chars: "abcdefghijklmnopqrstuvwxyz".into(),
            hint_bgcolor: "#1e1e2e".into(),
            hint_fgcolor: "#cdd6f4".into(),
            hint_border_radius: 4.0,
            hint_font: "monospace".into(),
            hint_font_size: 16.0,
            hint_grid_gap: 80,

            grid_color: "#89b4fa".into(),
            grid_border_size: 2,
            grid_min_size: 270.0,

            speed: 220,
            acceleration: 700,

            buttons: ["m".into(), ",".into(), ".".into()],

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
            let home_cfg = dirs::home_dir()
                .map(|h| h.join(".config/warpd-rs/config.toml"));
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
