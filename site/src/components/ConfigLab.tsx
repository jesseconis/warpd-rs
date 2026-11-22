import React, { useMemo, useState } from "react";
import { Check, Copy, ExternalLink, RotateCcw, Sliders } from "lucide-react";
import { INITIAL_CONFIG, WarpdConfig, serializeConfigToml } from "../lib/warpdEngine";

interface ConfigLabProps {
  config: WarpdConfig;
  onChange: React.Dispatch<React.SetStateAction<WarpdConfig>>;
}

const PRESETS: Array<{ name: string; patch: Partial<WarpdConfig> }> = [
  {
    name: "Catppuccin Mocha",
    patch: {
      hint_bgcolor: "#1e1e2e",
      hint_fgcolor: "#cdd6f4",
      grid_color: "#89b4fa",
      cursor_color: "#f38ba8",
      hint_border_radius: 4,
    },
  },
  {
    name: "Tokyo Night",
    patch: {
      hint_bgcolor: "#1a1b26",
      hint_fgcolor: "#c0caf5",
      grid_color: "#7aa2f7",
      cursor_color: "#f7768e",
      hint_border_radius: 6,
    },
  },
  {
    name: "Gruvbox Dark",
    patch: {
      hint_bgcolor: "#282828",
      hint_fgcolor: "#ebdbb2",
      grid_color: "#fabd2f",
      cursor_color: "#fb4934",
      hint_border_radius: 2,
    },
  },
  {
    name: "Home Row Labels",
    patch: {
      hint_chars: "asdfghjkl",
      hint_bgcolor: "#11111b",
      hint_fgcolor: "#a6e3a1",
      grid_color: "#94e2d5",
      cursor_color: "#fab387",
      hint_border_radius: 6,
    },
  },
];

export const ConfigLab: React.FC<ConfigLabProps> = ({ config, onChange }) => {
  const [copied, setCopied] = useState(false);
  const [bindTab, setBindTab] = useState<"hyprland" | "sway">("hyprland");

  const tomlOutput = useMemo(() => serializeConfigToml(config), [config]);

  const copyToml = async () => {
    try {
      await navigator.clipboard.writeText(tomlOutput);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      // ignore clipboard errors
    }
  };

  return (
    <section id="config-lab" className="section">
      <div className="section-head">
        <div>
          <span className="eyebrow">Live Configuration Customizer</span>
          <h2>Tune Your config.toml</h2>
          <p className="section-sub">
            Changes immediately update the interactive sandbox above. Copy directly into{" "}
            <code>~/.config/warpd-rs/config.toml</code> or generate defaults via{" "}
            <code>warpd-rs --generate-config</code>.
          </p>
        </div>
        <div className="playground-actions">
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => onChange(INITIAL_CONFIG)}
          >
            <RotateCcw size={14} /> Reset Defaults
          </button>
          <a href="/docs/configuration/" className="btn btn-ghost">
            Full Reference <ExternalLink size={14} />
          </a>
        </div>
      </div>

      <div className="preset-row">
        <span className="preset-label">
          <Sliders size={14} /> Theme Presets:
        </span>
        {PRESETS.map((p) => (
          <button
            key={p.name}
            type="button"
            className="preset-chip"
            onClick={() => onChange((prev) => ({ ...prev, ...p.patch }))}
          >
            <span
              className="preset-dots"
              aria-hidden="true"
            >
              <i style={{ background: p.patch.hint_bgcolor }} />
              <i style={{ background: p.patch.grid_color }} />
              <i style={{ background: p.patch.cursor_color }} />
            </span>
            {p.name}
          </button>
        ))}
      </div>

      <div className="config-lab-grid">
        <div className="config-controls">
          <div className="control-row">
            <label htmlFor="cfg-source">hint_source</label>
            <select
              id="cfg-source"
              value={config.hint_source}
              onChange={(e) =>
                onChange((p) => ({
                  ...p,
                  hint_source: e.target.value as WarpdConfig["hint_source"],
                }))
              }
            >
              <option value="static">static (uniform grid)</option>
              <option value="detect">detect (OpenCV screencopy)</option>
              <option value="stdin">stdin (wxh+x+y pipes)</option>
            </select>
          </div>

          <div className="control-row">
            <label htmlFor="cfg-chars">hint_chars</label>
            <input
              id="cfg-chars"
              type="text"
              value={config.hint_chars}
              onChange={(e) => onChange((p) => ({ ...p, hint_chars: e.target.value }))}
            />
          </div>

          <div className="control-colors">
            <label className="color-field">
              <span>hint_bgcolor</span>
              <input
                type="color"
                value={config.hint_bgcolor}
                onChange={(e) => onChange((p) => ({ ...p, hint_bgcolor: e.target.value }))}
              />
            </label>
            <label className="color-field">
              <span>hint_fgcolor</span>
              <input
                type="color"
                value={config.hint_fgcolor}
                onChange={(e) => onChange((p) => ({ ...p, hint_fgcolor: e.target.value }))}
              />
            </label>
            <label className="color-field">
              <span>grid_color</span>
              <input
                type="color"
                value={config.grid_color}
                onChange={(e) => onChange((p) => ({ ...p, grid_color: e.target.value }))}
              />
            </label>
            <label className="color-field">
              <span>cursor_color</span>
              <input
                type="color"
                value={config.cursor_color}
                onChange={(e) => onChange((p) => ({ ...p, cursor_color: e.target.value }))}
              />
            </label>
          </div>

          <div className="control-row">
            <label htmlFor="cfg-radius">
              hint_border_radius <code>{config.hint_border_radius}px</code>
            </label>
            <input
              id="cfg-radius"
              type="range"
              min={0}
              max={12}
              step={1}
              value={config.hint_border_radius}
              onChange={(e) =>
                onChange((p) => ({ ...p, hint_border_radius: Number(e.target.value) }))
              }
            />
          </div>

          <div className="control-row">
            <label htmlFor="cfg-mult">
              speed_modifier_multiplier <code>{config.speed_modifier_multiplier.toFixed(1)}×</code>
            </label>
            <input
              id="cfg-mult"
              type="range"
              min={1.5}
              max={8}
              step={0.5}
              value={config.speed_modifier_multiplier}
              onChange={(e) =>
                onChange((p) => ({ ...p, speed_modifier_multiplier: Number(e.target.value) }))
              }
            />
          </div>

          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={config.click_after_warp}
              onChange={(e) => onChange((p) => ({ ...p, click_after_warp: e.target.checked }))}
            />
            <span>
              <strong>click_after_warp</strong> — automatically click LMB after hint or grid warp
            </span>
          </label>

          {/* Compositor binds */}
          <div className="binds-card">
            <div className="binds-tabs">
              <button
                type="button"
                className={bindTab === "hyprland" ? "active" : ""}
                onClick={() => setBindTab("hyprland")}
              >
                Hyprland Binds
              </button>
              <button
                type="button"
                className={bindTab === "sway" ? "active" : ""}
                onClick={() => setBindTab("sway")}
              >
                Sway Binds
              </button>
            </div>
            <pre className="binds-pre">
              {bindTab === "hyprland"
                ? `bind = SUPER ALT, x, exec, warpd-rs --hint\nbind = SUPER ALT, g, exec, warpd-rs --grid\nbind = SUPER ALT, c, exec, warpd-rs --normal`
                : `bindsym $mod+Alt+x exec warpd-rs --hint\nbindsym $mod+Alt+g exec warpd-rs --grid\nbindsym $mod+Alt+c exec warpd-rs --normal`}
            </pre>
          </div>
        </div>

        <div className="config-preview">
          <div className="code-header">
            <span>~/.config/warpd-rs/config.toml</span>
            <button type="button" className="copy-pill" onClick={copyToml}>
              {copied ? <Check size={14} /> : <Copy size={14} />}
              {copied ? "Copied" : "Copy TOML"}
            </button>
          </div>
          <pre className="toml-pre">
            <code>{tomlOutput}</code>
          </pre>
        </div>
      </div>
    </section>
  );
};
