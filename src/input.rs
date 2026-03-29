///! Input helpers: key-name lookup, modifier detection, convenience wrappers
///! around the raw KeyEvent coming from the Wayland keyboard dispatch.
use log::warn;
use xkbcommon::xkb::{self, keysyms::KEY_NoSymbol};

/// Well-known XKB keysyms used throughout the mode loops.
#[allow(dead_code)]
pub mod keysyms {
    pub const ESCAPE: u32 = 0xff1b;
    pub const RETURN: u32 = 0xff0d;
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;

    // modifiers
    pub const SHIFT_L: u32 = 0xffe1;
    pub const SHIFT_R: u32 = 0xffe2;

    // hjkl movement (normal mode)
    pub const H: u32 = 0x68;
    pub const J: u32 = 0x6a;
    pub const K: u32 = 0x6b;
    pub const L: u32 = 0x6c;

    // grid quadrant selection
    pub const U: u32 = 0x75;
    pub const I: u32 = 0x69;

    // mouse buttons (default bindings)
    pub const M: u32 = 0x6d;
    pub const COMMA: u32 = 0x2c;
    pub const PERIOD: u32 = 0x2e;

    // mode switches
    pub const X: u32 = 0x78; // hint mode
    pub const G: u32 = 0x67; // grid mode
    pub const V: u32 = 0x76; // drag toggle
}

pub const DEFAULT_GRID_QUADRANT_KEYSYMS: [u32; 4] =
    [keysyms::U, keysyms::I, keysyms::J, keysyms::K];

pub const DEFAULT_NORMAL_MOVE_KEYSYMS: [u32; 4] = [keysyms::H, keysyms::J, keysyms::K, keysyms::L];

pub const DEFAULT_BUTTON_KEYSYMS: [u32; 3] = [keysyms::M, keysyms::COMMA, keysyms::PERIOD];

pub const SHIFT_KEYS: [u32; 2] = [keysyms::SHIFT_L, keysyms::SHIFT_R];

/// Convert a configuration binding string (single char or keysym name) into
/// an XKB keysym value.
pub fn binding_to_keysym(binding: &str) -> Option<u32> {
    let trimmed = binding.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.chars().count() == 1 {
        let ch = trimmed.chars().next().unwrap();
        let ascii = ch.to_ascii_lowercase();
        if ascii.is_ascii() {
            return Some(ascii as u32);
        }
    }

    match trimmed.to_ascii_lowercase().as_str() {
        "shift" | "shift_l" | "lshift" | "left_shift" => {
            return Some(keysyms::SHIFT_L);
        }
        "shift_r" | "rshift" | "right_shift" => {
            return Some(keysyms::SHIFT_R);
        }
        _ => {}
    }

    let sym = xkb::keysym_from_name(trimmed, xkb::KEYSYM_NO_FLAGS);
    let raw = sym.raw();
    if raw == KEY_NoSymbol {
        None
    } else {
        Some(raw)
    }
}

/// Resolve an array of bindings to keysyms, logging warnings and applying
/// fallbacks when parsing fails.
pub fn bindings_to_keysyms<const N: usize>(
    bindings: &[String; N],
    fallback: &[u32; N],
    field_name: &str,
) -> [u32; N] {
    let mut resolved = *fallback;
    for (idx, binding) in bindings.iter().enumerate() {
        if let Some(sym) = binding_to_keysym(binding) {
            resolved[idx] = sym;
        } else {
            warn!(
                "invalid binding '{}' for {}[{}]; using default",
                binding, field_name, idx
            );
        }
    }
    resolved
}
