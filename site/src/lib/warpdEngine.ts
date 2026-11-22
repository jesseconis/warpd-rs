import { CONFIG_DEFAULTS } from "../generated/repoMeta";

export type DemoMode = "hint-static" | "hint-detect" | "grid" | "normal";
export type DetectStage = "screencopy" | "edges" | "filter" | "hints";

export interface WarpdConfig {
  click_after_warp: boolean;
  hint_source: "static" | "detect" | "stdin";
  hint_chars: string;
  hint_bgcolor: string;
  hint_bg_opacity: number;
  hint_fgcolor: string;
  hint_border_radius: number;
  hint_font: string;
  hint_font_size: number;
  hint_grid_gap: number;
  grid_color: string;
  grid_border_size: number;
  grid_font_size: number;
  grid_min_size: number;
  grid_quadrant_keys: [string, string, string, string];
  speed: number;
  acceleration: number;
  normal_move_keys: [string, string, string, string];
  speed_modifier_key: string;
  speed_modifier_multiplier: number;
  cursor_color: string;
  cursor_size: number;
  crosshair_line_width: number;
  buttons: [string, string, string];
}

export const INITIAL_CONFIG: WarpdConfig = {
  click_after_warp: Boolean(CONFIG_DEFAULTS.click_after_warp),
  hint_source: (CONFIG_DEFAULTS.hint_source as WarpdConfig["hint_source"]) || "static",
  hint_chars: String(CONFIG_DEFAULTS.hint_chars),
  hint_bgcolor: String(CONFIG_DEFAULTS.hint_bgcolor),
  hint_bg_opacity: Number(CONFIG_DEFAULTS.hint_bg_opacity),
  hint_fgcolor: String(CONFIG_DEFAULTS.hint_fgcolor),
  hint_border_radius: Number(CONFIG_DEFAULTS.hint_border_radius),
  hint_font: String(CONFIG_DEFAULTS.hint_font),
  hint_font_size: Number(CONFIG_DEFAULTS.hint_font_size),
  hint_grid_gap: Number(CONFIG_DEFAULTS.hint_grid_gap),
  grid_color: String(CONFIG_DEFAULTS.grid_color),
  grid_border_size: Number(CONFIG_DEFAULTS.grid_border_size),
  grid_font_size: Number(CONFIG_DEFAULTS.grid_font_size),
  grid_min_size: Number(CONFIG_DEFAULTS.grid_min_size),
  grid_quadrant_keys: [...CONFIG_DEFAULTS.grid_quadrant_keys] as [string, string, string, string],
  speed: Number(CONFIG_DEFAULTS.speed),
  acceleration: Number(CONFIG_DEFAULTS.acceleration),
  normal_move_keys: [...CONFIG_DEFAULTS.normal_move_keys] as [string, string, string, string],
  speed_modifier_key: String(CONFIG_DEFAULTS.speed_modifier_key),
  speed_modifier_multiplier: Number(CONFIG_DEFAULTS.speed_modifier_multiplier),
  cursor_color: String(CONFIG_DEFAULTS.cursor_color),
  cursor_size: Number(CONFIG_DEFAULTS.cursor_size),
  crosshair_line_width: Number(CONFIG_DEFAULTS.crosshair_line_width),
  buttons: [...CONFIG_DEFAULTS.buttons] as [string, string, string],
};

export interface HintBox {
  id: string;
  /** Center X as percentage (0..100) of simulated desktop */
  cx: number;
  /** Center Y as percentage (0..100) of simulated desktop */
  cy: number;
  /** Optional target box bounds in percentage for OpenCV detect mode */
  x?: number;
  y?: number;
  w?: number;
  h?: number;
  label: string;
  targetName?: string;
  targetAction?: string;
}

export interface DesktopTarget {
  id: string;
  name: string;
  action: string;
  /** Percentage bounding box inside the simulated 16:9 desktop */
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface PrunedContainer {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
  reason: string;
}

export interface GridSelection {
  x: number;
  y: number;
  w: number;
  h: number;
  depth: number;
}

/**
 * Exact TypeScript mirror of `generate_labels` in `src/hint.rs`:
 * Uses the shortest uniform length that covers `count` targets using unique
 * ASCII letters in configured order, falling back to a-z if invalid.
 */
export function generateLabels(chars: string, count: number): string[] {
  if (count <= 0) return [];
  const alphabet: string[] = [];
  for (const ch of chars) {
    if (/^[a-zA-Z]$/.test(ch)) {
      const lower = ch.toLowerCase();
      if (!alphabet.includes(lower)) {
        alphabet.push(lower);
      }
    }
  }
  const effective =
    alphabet.length === 0 || (alphabet.length === 1 && count > 1)
      ? "abcdefghijklmnopqrstuvwxyz".split("")
      : alphabet;

  const radix = effective.length;
  let width = 1;
  let capacity = radix;
  while (capacity < count) {
    width += 1;
    capacity *= radix;
  }

  const labels: string[] = [];
  for (let i = 0; i < count; i += 1) {
    let idx = i;
    const buf = new Array<string>(width);
    for (let pos = width - 1; pos >= 0; pos -= 1) {
      buf[pos] = effective[idx % radix];
      idx = Math.floor(idx / radix);
    }
    labels.push(buf.join(""));
  }
  return labels;
}

export function generateStaticHints(
  hintChars: string,
  cols = 9,
  rows = 5
): HintBox[] {
  const total = cols * rows;
  const labels = generateLabels(hintChars, total);
  const out: HintBox[] = [];
  for (let r = 0; r < rows; r += 1) {
    for (let c = 0; c < cols; c += 1) {
      const idx = r * cols + c;
      const cx = ((c + 0.5) / cols) * 92 + 4;
      const cy = ((r + 0.5) / rows) * 86 + 9;
      out.push({
        id: `static-${idx}`,
        cx,
        cy,
        label: labels[idx],
      });
    }
  }
  return out;
}

export const DESKTOP_TARGETS: DesktopTarget[] = [
  // Waybar top bar
  { id: "ws-1", name: "Workspace 1", action: "Switched to workspace 1 (Code)", x: 1.5, y: 1.2, w: 3.4, h: 4.2 },
  { id: "ws-2", name: "Workspace 2", action: "Switched to workspace 2 (Web)", x: 5.4, y: 1.2, w: 3.4, h: 4.2 },
  { id: "ws-3", name: "Workspace 3", action: "Switched to workspace 3 (Term)", x: 9.3, y: 1.2, w: 3.4, h: 4.2 },
  { id: "wb-net", name: "Waybar Wi-Fi", action: "Opened Network status (1.2 Gbps)", x: 82.0, y: 1.2, w: 7.5, h: 4.2 },
  { id: "wb-clock", name: "Waybar Clock", action: "Toggled calendar popover", x: 90.5, y: 1.2, w: 8.0, h: 4.2 },

  // Browser window (left pane)
  { id: "tab-docs", name: "Tab: warpd.jesse.rs", action: "Focused warpd.jesse.rs tab", x: 3.2, y: 9.5, w: 14.5, h: 4.8 },
  { id: "tab-gh", name: "Tab: GitHub PR", action: "Focused GitHub repository tab", x: 18.4, y: 9.5, w: 13.0, h: 4.8 },
  { id: "url-bar", name: "Address Bar", action: "Focused browser URL input", x: 6.5, y: 15.8, w: 34.0, h: 4.6 },
  { id: "btn-reload", name: "Reload Page", action: "Reloaded active page", x: 2.8, y: 15.8, w: 3.1, h: 4.6 },
  { id: "btn-star", name: "Star Repo", action: "Starred jesseconis/warpd-rs ★", x: 41.2, y: 15.8, w: 5.8, h: 4.6 },
  { id: "card-hint", name: "Hint Mode Pill", action: "Selected Hint Mode overview card", x: 4.2, y: 39.5, w: 13.2, h: 6.8 },
  { id: "card-grid", name: "Grid Mode Pill", action: "Selected Grid Mode overview card", x: 18.6, y: 39.5, w: 13.2, h: 6.8 },
  { id: "card-normal", name: "Normal Mode Pill", action: "Selected Normal Mode overview card", x: 33.0, y: 39.5, w: 13.2, h: 6.8 },
  { id: "btn-release", name: "Download v0.2.2", action: "Triggered release binary download", x: 4.2, y: 52.0, w: 17.5, h: 6.2 },
  { id: "btn-docs-link", name: "Open Docs", action: "Navigated to /docs/ reference", x: 23.0, y: 52.0, w: 13.5, h: 6.2 },
  { id: "toggle-opencv", name: "OpenCV Toggle", action: "Toggled OpenCV detect pipeline", x: 4.2, y: 74.5, w: 18.5, h: 5.8 },
  { id: "btn-copy-cfg", name: "Copy TOML", action: "Copied config.toml to Wayland clipboard", x: 24.5, y: 74.5, w: 14.5, h: 5.8 },

  // Code Editor (top right pane)
  { id: "file-main", name: "src/main.rs", action: "Opened src/main.rs in editor", x: 52.0, y: 15.6, w: 11.2, h: 4.5 },
  { id: "file-hint", name: "src/hint.rs", action: "Opened src/hint.rs in editor", x: 52.0, y: 21.2, w: 11.2, h: 4.5 },
  { id: "file-detect", name: "target_detection.rs", action: "Opened src/target_detection.rs", x: 52.0, y: 26.8, w: 11.2, h: 4.5 },
  { id: "btn-run-test", name: "Run cargo test", action: "Ran cargo test --features opencv ✓", x: 84.5, y: 9.5, w: 12.2, h: 4.6 },

  // Terminal (bottom right pane)
  { id: "term-hint", name: "Run --hint", action: "Executed warpd-rs --hint", x: 52.4, y: 69.5, w: 13.5, h: 5.4 },
  { id: "term-grid", name: "Run --grid", action: "Executed warpd-rs --grid", x: 67.2, y: 69.5, w: 13.5, h: 5.4 },
  { id: "term-clear", name: "Clear Logs", action: "Cleared WAYLAND_DEBUG output", x: 82.0, y: 69.5, w: 13.5, h: 5.4 },
];

export const PRUNED_CONTAINERS: PrunedContainer[] = [
  { id: "p-browser", x: 1.8, y: 8.2, w: 46.5, h: 88.5, reason: "h >= MAX_RECT_HEIGHT (window container)" },
  { id: "p-editor", x: 50.4, y: 8.2, w: 47.8, h: 44.0, reason: "h >= MAX_RECT_HEIGHT (editor pane)" },
  { id: "p-term", x: 50.4, y: 54.4, w: 47.8, h: 42.3, reason: "h >= MAX_RECT_HEIGHT (terminal pane)" },
  { id: "p-hero-box", x: 3.5, y: 23.2, w: 43.2, h: 14.0, reason: "outer banner container pruned" },
  { id: "p-sidebar", x: 51.4, y: 14.4, w: 12.6, h: 36.2, reason: "sidebar container pruned" },
];

export function generateDetectHints(hintChars: string): HintBox[] {
  const labels = generateLabels(hintChars, DESKTOP_TARGETS.length);
  return DESKTOP_TARGETS.map((t, idx) => ({
    id: t.id,
    cx: t.x + t.w / 2,
    cy: t.y + t.h / 2,
    x: t.x,
    y: t.y,
    w: t.w,
    h: t.h,
    label: labels[idx],
    targetName: t.name,
    targetAction: t.action,
  }));
}

export function subdivideGrid(sel: GridSelection, quadrant: 0 | 1 | 2 | 3): GridSelection {
  const hw = sel.w / 2;
  const hh = sel.h / 2;
  let { x, y } = sel;
  if (quadrant === 1) x += hw;
  if (quadrant === 2) y += hh;
  if (quadrant === 3) {
    x += hw;
    y += hh;
  }
  return { x, y, w: hw, h: hh, depth: sel.depth + 1 };
}

export function hitTestTarget(cxPercent: number, cyPercent: number): DesktopTarget | null {
  for (const t of DESKTOP_TARGETS) {
    if (
      cxPercent >= t.x - 1.2 &&
      cxPercent <= t.x + t.w + 1.2 &&
      cyPercent >= t.y - 1.2 &&
      cyPercent <= t.y + t.h + 1.2
    ) {
      return t;
    }
  }
  return null;
}

export function serializeConfigToml(cfg: WarpdConfig): string {
  const fmtArr = (arr: string[]) =>
    `[\n${arr.map((k) => `    ${JSON.stringify(k)},`).join("\n")}\n]`;

  return [
    `click_after_warp = ${cfg.click_after_warp}`,
    `hint_source = ${JSON.stringify(cfg.hint_source)}`,
    `hint_chars = ${JSON.stringify(cfg.hint_chars)}`,
    `hint_bgcolor = ${JSON.stringify(cfg.hint_bgcolor)}`,
    `hint_bg_opacity = ${cfg.hint_bg_opacity.toFixed(1)}`,
    `hint_fgcolor = ${JSON.stringify(cfg.hint_fgcolor)}`,
    `hint_border_radius = ${cfg.hint_border_radius.toFixed(1)}`,
    `hint_font = ${JSON.stringify(cfg.hint_font)}`,
    `hint_font_size = ${cfg.hint_font_size.toFixed(1)}`,
    `hint_grid_gap = ${cfg.hint_grid_gap}`,
    `grid_color = ${JSON.stringify(cfg.grid_color)}`,
    `grid_border_size = ${cfg.grid_border_size}`,
    `grid_font_size = ${cfg.grid_font_size.toFixed(1)}`,
    `grid_min_size = ${cfg.grid_min_size.toFixed(1)}`,
    `grid_quadrant_keys = ${fmtArr(cfg.grid_quadrant_keys)}`,
    `speed = ${cfg.speed}`,
    `acceleration = ${cfg.acceleration}`,
    `normal_move_keys = ${fmtArr(cfg.normal_move_keys)}`,
    `speed_modifier_key = ${JSON.stringify(cfg.speed_modifier_key)}`,
    `speed_modifier_multiplier = ${cfg.speed_modifier_multiplier.toFixed(1)}`,
    `cursor_color = ${JSON.stringify(cfg.cursor_color)}`,
    `cursor_size = ${cfg.cursor_size}`,
    `crosshair_line_width = ${cfg.crosshair_line_width}`,
    `buttons = ${fmtArr(cfg.buttons)}`,
  ].join("\n");
}
