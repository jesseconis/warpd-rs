# warpd-rs

A modal keyboard-driven virtual pointer for **Wayland**, rewritten in Rust.

Inspired by [warpd](https://github.com/rvaiya/warpd) — this is a
Wayland-first implementation that targets wlroots-based compositors
(Sway, Hyprland, etc.).

## Modes


### Hint

#### Static
   <img src="docs/tile_static.gif" height="450px"/>

#### Detect
   <img src="docs/tile_detect.gif" height="450px"/>

### Grid
   <img src="docs/grid.gif" height="450px"/>

### Normal
   <img src="docs/normal.gif" height="450px"/>

## Debug

`RUST_LOG=debug ./warpd-rs --<mode>` 

For troubleshooting compositor/wayland issues `WAYLAND_DEBUG=1` 



## Build

```bash
cargo build --release
```

With auto-detection support:

```bash
cargo build --release --features opencv
```

## Usage

warpd-rs is designed to be invoked directly by your compositor's hotkey
system — it runs a single mode then exits (oneshot, like the original
warpd on Wayland).

```bash
warpd-rs --hint      # hint mode
warpd-rs --grid      # grid mode
warpd-rs --normal    # normal (discrete) mode
warpd-rs --version   # print version and compiled runtime features
warpd-rs --hint --config ./config.example.toml
```

`--version` prints feature support, for example:

```text
warpd-rs 0.1.0 (opencv)
```

### Target Detection Invocation

1. Build with OpenCV support:

```bash
cargo build --release --features opencv
```

2. Set detection mode in config (for example in `~/.config/warpd-rs/config.toml`):

```toml
hint_source = "detect"
```

3. Run hint mode:

```bash
./target/release/warpd-rs --hint
```

### Hyprland

```ini
bind = SUPER ALT, x, exec, warpd-rs --hint
bind = SUPER ALT, g, exec, warpd-rs --grid
bind = SUPER ALT, c, exec, warpd-rs --normal
```

# Usage

## Modes

### Hint Mode (`--hint`)

1. A grid of labelled boxes appears over the screen
2. Type characters to filter — matching prefix is dimmed, remaining chars highlighted
3. When one hint remains the cursor warps to its centre
4. Press **Escape** to cancel, **Backspace** to undo a character

Hint targets are selected by `hint_source`:

- `grid` (default): regular grid-based hints across the monitor
- `stdin`: read target areas from stdin, one `wxh+x+y` rectangle per line
- `detect`: run OpenCV-based target detection on a screencopy frame (requires
   building with `--features opencv` and compositor support for
   `wlr-screencopy-unstable-v1`)

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

Place a TOML file at `~/.config/warpd-rs/config.toml` [an example/default is provided](./example.config.toml)



## License

MIT
