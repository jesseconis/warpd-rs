import React, { useState } from "react";
import { Eye, Film, ExternalLink } from "lucide-react";
import { DETECT_STATS } from "../generated/repoMeta";

type CaptureId = "tile_detect" | "tile_static" | "grid" | "normal" | "overlay";

const CAPTURES: Array<{
  id: CaptureId;
  title: string;
  cmd: string;
  desc: string;
  src: string;
  kind: "video" | "image";
}> = [
  {
    id: "tile_detect",
    title: "Hint — OpenCV Detect",
    cmd: 'warpd-rs --hint  # hint_source = "detect"',
    desc: "Captures the focused monitor via zwlr_screencopy_manager_v1 and snaps prefix-free hints onto detected UI elements.",
    src: "/docs/tile_detect.mp4",
    kind: "video",
  },
  {
    id: "tile_static",
    title: "Hint — Static Grid",
    cmd: 'warpd-rs --hint  # hint_source = "static"',
    desc: "Zero-latency uniform interval hint grid covering the entire monitor with configurable hint_grid_gap.",
    src: "/docs/tile_static.mp4",
    kind: "video",
  },
  {
    id: "grid",
    title: "Grid Mode",
    cmd: "warpd-rs --grid",
    desc: "Recursive 2×2 quadrant subdivision with u/i/j/k until the target cell drops below grid_min_size.",
    src: "/docs/grid.mp4",
    kind: "video",
  },
  {
    id: "normal",
    title: "Normal Mode",
    cmd: "warpd-rs --normal",
    desc: "120fps Cairo crosshair controlled via h/j/k/l with Shift acceleration and direct mode switching.",
    src: "/docs/normal.mp4",
    kind: "video",
  },
  {
    id: "overlay",
    title: "OpenCV Debug Overlay",
    cmd: "WARPDRS_DETECT_DEBUG=1 warpd-rs --hint",
    desc: `Real debug capture from ${DETECT_STATS.monitor} (${DETECT_STATS.frame}): ${DETECT_STATS.totalRects.toLocaleString()} raw contours filtered to ${DETECT_STATS.keptFinal.toLocaleString()} green forwarded targets.`,
    src: "/docs/05_rects_overlay.png",
    kind: "image",
  },
];

export const RealCaptures: React.FC = () => {
  const [selected, setSelected] = useState<CaptureId>("tile_detect");
  const current = CAPTURES.find((c) => c.id === selected) || CAPTURES[0];

  return (
    <section id="captures" className="section">
      <div className="section-head">
        <div>
          <span className="eyebrow">Real Wayland Recordings</span>
          <h2>Recorded Directly on Hyprland</h2>
          <p className="section-sub">
            Actual screen captures and OpenCV diagnostics pulled automatically from the repository{" "}
            <code>docs/</code> directory during every build.
          </p>
        </div>
        <a href="/docs/recordings/" className="btn btn-ghost">
          All Recordings &amp; Stats <ExternalLink size={14} />
        </a>
      </div>

      <div className="captures-layout">
        <div className="captures-list" role="tablist" aria-label="Real recordings">
          {CAPTURES.map((item) => (
            <button
              key={item.id}
              type="button"
              role="tab"
              aria-selected={selected === item.id}
              className={`capture-item ${selected === item.id ? "active" : ""}`}
              onClick={() => setSelected(item.id)}
            >
              <div className="capture-item-top">
                {item.kind === "video" ? <Film size={15} /> : <Eye size={15} />}
                <span>{item.title}</span>
              </div>
              <code>{item.cmd}</code>
            </button>
          ))}

          <div className="stats-mini-card">
            <div className="stats-mini-label">docs/stats.txt snapshot</div>
            <div className="stats-mini-grid">
              <div>
                <span className="stat-val">{DETECT_STATS.frame}</span>
                <span className="stat-key">{DETECT_STATS.monitor}</span>
              </div>
              <div>
                <span className="stat-val">{DETECT_STATS.totalRects.toLocaleString()}</span>
                <span className="stat-key">Raw Rects</span>
              </div>
              <div>
                <span className="stat-val stat-green">{DETECT_STATS.keptFinal.toLocaleString()}</span>
                <span className="stat-key">Kept Hints</span>
              </div>
              <div>
                <span className="stat-val">{DETECT_STATS.kernel}</span>
                <span className="stat-key">Dilate Kernel</span>
              </div>
            </div>
          </div>
        </div>

        <div className="capture-stage">
          <div className="capture-stage-bar">
            <span className="dot red" />
            <span className="dot yellow" />
            <span className="dot green" />
            <code className="capture-stage-cmd">{current.cmd}</code>
          </div>
          <div className="capture-media-wrap">
            {current.kind === "video" ? (
              <video
                key={current.src}
                src={current.src}
                autoPlay
                loop
                muted
                playsInline
                controls
                className="capture-media"
              />
            ) : (
              <img
                key={current.src}
                src={current.src}
                alt="OpenCV detection candidate overlay showing pruned red rectangles and kept green rectangles"
                className="capture-media"
              />
            )}
          </div>
          <p className="capture-caption">{current.desc}</p>
        </div>
      </div>
    </section>
  );
};
