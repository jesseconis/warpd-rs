# warpd-rs

A modal keyboard-driven virtual pointer for **Wayland**, rewritten in Rust.

Inspired by [warpd](https://github.com/rvaiya/warpd) — this is a
Wayland-first implementation that targets wlroots-based compositors
(Sway, Hyprland, etc.).

## Features

- **Hint mode** — screen fills with labelled targets; type to warp instantly
- **Grid mode** — recursive quadrant subdivision (u/i/j/k) for precise positioning
- **Normal mode** — hjkl continuous cursor movement with crosshair overlay
- First-class Wayland support via wlroots protocols
- Cairo-based overlay rendering with configurable colours and fonts
- XKB keyboard handling
- TOML configuration file

## Dependencies

System libraries (install via your package manager):

```
# Arch
sudo pacman -S wayland cairo pango libxkbcommon

# Debian/Ubuntu
sudo apt install libwayland-dev libcairo2-dev libxkbcommon-dev

# Fedora
sudo dnf install wayland-devel cairo-devel libxkbcommon-devel
```

Rust toolchain (1.70+):

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Build

```bash
cargo build --release
```

The binary will be at `target/release/warpd-rs`.

## Usage

warpd-rs is designed to be invoked directly by your compositor's hotkey
system — it runs a single mode then exits (oneshot, like the original
warpd on Wayland).

```bash
warpd-rs --hint      # hint mode
warpd-rs --grid      # grid mode
warpd-rs --normal    # normal (discrete) mode
warpd-rs --hint --config ./config.example.toml
```

### Hyprland

```ini
bind = SUPER ALT, x, exec, warpd-rs --hint
bind = SUPER ALT, g, exec, warpd-rs --grid
bind = SUPER ALT, c, exec, warpd-rs --normal
```

### Sway

```ini
bindsym Mod4+Mod1+x exec warpd-rs --hint
bindsym Mod4+Mod1+g exec warpd-rs --grid
bindsym Mod4+Mod1+c exec warpd-rs --normal
```

## Modes

### Hint Mode (`--hint`)

1. A grid of labelled boxes appears over the screen
2. Type characters to filter — matching prefix is dimmed, remaining chars highlighted
3. When one hint remains the cursor warps to its centre
4. Press **Escape** to cancel, **Backspace** to undo a character

### Grid Mode (`--grid`)

1. The screen is divided into four quadrants with a crosshair
2. Press **u** (top-left), **i** (top-right), **j** (bottom-left), **k** (bottom-right) to subdivide
3. Repeat until precise — the cursor warps when the cell is small enough
4. Press **m** to left-click at the current centre, **Escape** to cancel

### Normal Mode (`--normal`)

1. A crosshair with a cursor dot appears at the screen centre
2. Hold **h/j/k/l** for continuous movement
3. Press **m** for left-click, **,** for middle-click, **.** for right-click
4. Press **x** to switch to hint mode, **g** to switch to grid mode
5. Press **Escape** to cancel

## Configuration

Place a TOML file at `~/.config/warpd-rs/config.toml`:

You can also pass an explicit config file path with `--config /path/to/config.toml`.
When `--config` is provided, default search locations are not used.

```toml
# Characters used for hint labels (order matters)
hint_chars = "aoeuidhtns"

# Visual appearance
hint_bgcolor = "#1e1e2e"
hint_fgcolor = "#cdd6f4"
hint_border_radius = 4.0
hint_font = "monospace"
hint_font_size = 16.0
hint_grid_gap = 80

# Grid mode
grid_color = "#89b4fa"
grid_border_size = 2
grid_min_size = 30.0

# Normal mode movement speed
speed = 220

# Cursor appearance
cursor_color = "#f38ba8"
cursor_size = 7
```

Any missing keys use sensible defaults (Catppuccin Mocha palette).

## Architecture

```
src/
├── main.rs           CLI parsing, mode orchestration, event loop
├── config/mod.rs     TOML config loading, colour parsing
├── wayland/mod.rs    Wayland connection, globals, overlays, virtual pointer
├── hint/mod.rs       Hint generation, Cairo drawing, grid mode drawing
└── input/mod.rs      Key event helpers, keysym constants
```

### Wayland protocols used

| Protocol | Purpose |
|---|---|
| `wl_compositor` | Surface creation |
| `wl_shm` | Shared-memory buffer allocation |
| `wl_seat` / `wl_keyboard` | Input capture |
| `wl_output` | Monitor enumeration |
| `zwlr_layer_shell_v1` | Fullscreen overlay above all windows |
| `zwlr_virtual_pointer_manager_v1` | Synthetic mouse movement & clicks |
| `zxdg_output_manager_v1` | Logical monitor geometry |

### How it works

1. On launch, connects to the Wayland compositor and binds globals
2. Enumerates all monitors via `wl_output` + `zxdg_output_v1`
3. Creates a full-screen overlay on the target monitor using `zwlr_layer_shell_v1`
   with exclusive keyboard interactivity
4. Allocates a POSIX shared-memory buffer and creates a Cairo surface over it
5. Draws mode-specific visuals (hints / grid / cursor) into the buffer
6. Processes keyboard input via `wl_keyboard` + xkbcommon
7. On selection, destroys the overlay and warps the pointer via `zwlr_virtual_pointer_v1`

## Differences from the original warpd

- **Rust** instead of C
- **Wayland-only** — no X11, macOS, or Windows backends
- **Oneshot only** — no daemon mode (compositor handles hotkeys)
- **Cairo** for drawing (same as original's Wayland path)
- **TOML** config instead of custom format
- Hint labels use **prefix-matching** (type-ahead filter)

## License

MIT
