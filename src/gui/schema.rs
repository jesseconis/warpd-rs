use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Hint,
    Grid,
    Normal,
    Cursor,
    Mouse,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::Hint,
        Section::Grid,
        Section::Normal,
        Section::Cursor,
        Section::Mouse,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Section::Hint => "Hint mode",
            Section::Grid => "Grid mode",
            Section::Normal => "Normal movement",
            Section::Cursor => "Cursor appearance",
            Section::Mouse => "Mouse and click behavior",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Section::Hint => "Target generation, labeling, colors, and spacing.",
            Section::Grid => "Quadrant navigation keys and grid presentation.",
            Section::Normal => "Pointer motion, bindings, and acceleration.",
            Section::Cursor => "Normal-mode cursor color and sizing.",
            Section::Mouse => "Button bindings and post-warp click behavior.",
        }
    }
}

pub struct SliderSpecF64 {
    pub label: &'static str,
    pub help: &'static str,
    pub range: RangeInclusive<f64>,
}

pub struct SliderSpecU32 {
    pub label: &'static str,
    pub help: &'static str,
    pub range: RangeInclusive<u32>,
}

pub struct TextSpec {
    pub label: &'static str,
    pub help: &'static str,
}

pub struct BoolSpec {
    pub label: &'static str,
    pub help: &'static str,
}

pub struct ChoiceSpec {
    pub label: &'static str,
    pub help: &'static str,
    pub options: &'static [&'static str],
}

pub const HINT_SOURCE: ChoiceSpec = ChoiceSpec {
    label: "Hint source",
    help: "How hint targets are generated.",
    options: &["static", "detect"],
};

pub const HINT_CHARS: TextSpec = TextSpec {
    label: "Hint characters",
    help: "Ordered alphabet used to generate hint labels.",
};

pub const HINT_BGCOLOR: TextSpec = TextSpec {
    label: "Hint background color",
    help: "Hex color used for hint background fill.",
};

pub const HINT_FGCOLOR: TextSpec = TextSpec {
    label: "Hint text color",
    help: "Hex color used for hint text.",
};

pub const HINT_BG_OPACITY: SliderSpecF64 = SliderSpecF64 {
    label: "Hint background opacity",
    help: "Opacity multiplier applied to the hint background.",
    range: 0.0..=1.0,
};

pub const HINT_BORDER_RADIUS: SliderSpecF64 = SliderSpecF64 {
    label: "Hint border radius",
    help: "Corner radius in pixels for hint boxes.",
    range: 0.0..=32.0,
};

pub const HINT_FONT: TextSpec = TextSpec {
    label: "Hint font",
    help: "Font family used for hint labels.",
};

pub const HINT_FONT_SIZE: SliderSpecF64 = SliderSpecF64 {
    label: "Hint font size",
    help: "Font size in pixels for hint labels.",
    range: 8.0..=64.0,
};

pub const HINT_GRID_GAP: SliderSpecU32 = SliderSpecU32 {
    label: "Hint grid gap",
    help: "Spacing in pixels between generated hint targets.",
    range: 16..=240,
};

pub const GRID_COLOR: TextSpec = TextSpec {
    label: "Grid color",
    help: "Hex color used for the recursive grid overlay.",
};

pub const GRID_BORDER_SIZE: SliderSpecU32 = SliderSpecU32 {
    label: "Grid border size",
    help: "Stroke width used for the grid border.",
    range: 1..=12,
};

pub const GRID_FONT_SIZE: SliderSpecF64 = SliderSpecF64 {
    label: "Grid font size",
    help: "Font size in pixels for quadrant labels.",
    range: 12.0..=96.0,
};

pub const GRID_MIN_SIZE: SliderSpecF64 = SliderSpecF64 {
    label: "Grid minimum size",
    help: "Auto-select center once the grid cell shrinks below this size.",
    range: 32.0..=1200.0,
};

pub const SPEED: SliderSpecU32 = SliderSpecU32 {
    label: "Pointer speed",
    help: "Base normal-mode cursor speed.",
    range: 1..=2000,
};

pub const ACCELERATION: SliderSpecU32 = SliderSpecU32 {
    label: "Pointer acceleration",
    help: "Acceleration used while holding movement keys.",
    range: 0..=5000,
};

pub const SPEED_MODIFIER_KEY: TextSpec = TextSpec {
    label: "Speed modifier key",
    help: "Keysym or single character that boosts cursor speed when held.",
};

pub const SPEED_MODIFIER_MULTIPLIER: SliderSpecF64 = SliderSpecF64 {
    label: "Speed modifier multiplier",
    help: "Multiplier applied while the speed modifier key is active.",
    range: 1.0..=20.0,
};

pub const CURSOR_COLOR: TextSpec = TextSpec {
    label: "Cursor color",
    help: "Hex color used for the normal-mode cursor overlay.",
};

pub const CURSOR_SIZE: SliderSpecU32 = SliderSpecU32 {
    label: "Cursor size",
    help: "Cursor dot radius in pixels.",
    range: 1..=40,
};

pub const CROSSHAIR_LINE_WIDTH: SliderSpecU32 = SliderSpecU32 {
    label: "Crosshair line width",
    help: "Stroke width of the normal-mode crosshair.",
    range: 1..=12,
};

pub const CLICK_AFTER_WARP: BoolSpec = BoolSpec {
    label: "Click after warp",
    help: "Automatically emit a left click after hint/grid warp completes.",
};

pub const GRID_KEY_LABELS: [&str; 4] = ["Top-left", "Top-right", "Bottom-left", "Bottom-right"];
pub const MOVE_KEY_LABELS: [&str; 4] = ["Left", "Down", "Up", "Right"];
pub const BUTTON_LABELS: [&str; 3] = ["Left click", "Middle click", "Right click"];
