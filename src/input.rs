///! Input helpers: key-name lookup, modifier detection, convenience wrappers
///! around the raw KeyEvent coming from the Wayland keyboard dispatch.

use crate::wayland::KeyEvent;
use wayland_client::protocol::wl_keyboard::KeyState;

/// Well-known XKB keysyms used throughout the mode loops.
pub mod keysyms {
    pub const ESCAPE: u32 = 0xff1b;
    pub const RETURN: u32 = 0xff0d;
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;

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

/// Returns true if this is a key-down event.
pub fn is_press(event: &KeyEvent) -> bool {
    event.state == KeyState::Pressed
}

/// Try to get a single lowercase ASCII character from the event.
pub fn to_char(event: &KeyEvent) -> Option<char> {
    event.utf8.as_ref().and_then(|s| {
        let mut chars = s.chars();
        let c = chars.next()?;
        if chars.next().is_some() {
            return None; // multi-char
        }
        Some(c.to_ascii_lowercase())
    })
}
