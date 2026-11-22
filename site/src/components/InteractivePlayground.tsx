import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Crosshair,
  Grid2x2,
  Keyboard,
  Play,
  RotateCcw,
  ScanEye,
  Sparkles,
  Square,
  Terminal,
} from "lucide-react";
import {
  DESKTOP_TARGETS,
  DemoMode,
  DetectStage,
  GridSelection,
  PRUNED_CONTAINERS,
  WarpdConfig,
  generateDetectHints,
  generateStaticHints,
  hitTestTarget,
  subdivideGrid,
} from "../lib/warpdEngine";

interface InteractivePlaygroundProps {
  config: WarpdConfig;
}

interface ClickRipple {
  id: number;
  x: number;
  y: number;
  button: "LMB" | "MMB" | "RMB";
}

interface TelemetryEvent {
  id: number;
  ts: string;
  line: string;
  accent?: "blue" | "pink" | "green" | "mauve";
}

const VIRTUAL_W = 2560;
const VIRTUAL_H = 1440;

const MODE_TABS: Array<{
  id: DemoMode;
  label: string;
  cmd: string;
  icon: React.ReactNode;
  blurb: string;
}> = [
  {
    id: "hint-static",
    label: "Hint (Static)",
    cmd: "warpd-rs --hint",
    icon: <Keyboard size={15} />,
    blurb: "Uniform prefix-free hint grid. Type letters to filter and warp.",
  },
  {
    id: "hint-detect",
    label: "Hint (OpenCV Detect)",
    cmd: "warpd-rs --hint  # hint_source = \"detect\"",
    icon: <ScanEye size={15} />,
    blurb: "Screencopy + Canny edge contours snap hints straight onto UI controls.",
  },
  {
    id: "grid",
    label: "Grid Mode",
    cmd: "warpd-rs --grid",
    icon: <Grid2x2 size={15} />,
    blurb: "Recursive 2×2 quadrant subdivision with u / i / j / k, or m to click.",
  },
  {
    id: "normal",
    label: "Normal Mode",
    cmd: "warpd-rs --normal",
    icon: <Crosshair size={15} />,
    blurb: "Continuous 120fps h/j/k/l steering, Shift speed boost, m / , / . clicks.",
  },
];

const DETECT_STAGES: Array<{ id: DetectStage; label: string; step: string }> = [
  { id: "screencopy", label: "Screencopy", step: "01" },
  { id: "edges", label: "Canny + Dilate", step: "02" },
  { id: "filter", label: "Prune vs Keep", step: "03" },
  { id: "hints", label: "Live Hints", step: "04" },
];

export const InteractivePlayground: React.FC<InteractivePlaygroundProps> = ({ config }) => {
  const [mode, setMode] = useState<DemoMode>("hint-detect");
  const [detectStage, setDetectStage] = useState<DetectStage>("hints");
  const [typed, setTyped] = useState("");
  const [gridSel, setGridSel] = useState<GridSelection>({ x: 0, y: 0, w: 100, h: 100, depth: 0 });
  const [cursor, setCursor] = useState({ x: 50, y: 50 });
  const [ripples, setRipples] = useState<ClickRipple[]>([]);
  const [activeTarget, setActiveTarget] = useState<string>("Ready — click Auto-Play or focus and type keys");
  const [activeKeyFlash, setActiveKeyFlash] = useState<string | null>(null);
  const [autoPlaying, setAutoPlaying] = useState(false);
  const [focused, setFocused] = useState(false);
  const [activeWs, setActiveWs] = useState(1);
  const [activeEditorFile, setActiveEditorFile] = useState("src/hint.rs");
  const [starred, setStarred] = useState(false);
  const [shiftHeld, setShiftHeld] = useState(false);
  const [logs, setLogs] = useState<TelemetryEvent[]>([
    { id: 1, ts: "00.004", line: "-> wl_registry.bind(zwlr_layer_shell_v1, v5)", accent: "blue" },
    { id: 2, ts: "00.008", line: "-> wl_registry.bind(zwlr_virtual_pointer_manager_v1, v2)", accent: "blue" },
    { id: 3, ts: "00.014", line: "-> zwlr_screencopy_manager_v1.capture_output(HDMI-A-2)", accent: "mauve" },
    { id: 4, ts: "00.029", line: "opencv: 24 targets kept, 5 containers pruned (kernel 4x3)", accent: "green" },
  ]);

  const stageRef = useRef<HTMLDivElement>(null);
  const heldDirsRef = useRef<Set<"left" | "down" | "up" | "right">>(new Set());
  const shiftRef = useRef(false);
  const cursorRef = useRef(cursor);
  cursorRef.current = cursor;
  const logCounterRef = useRef(10);
  const autoTimersRef = useRef<number[]>([]);

  const pushLog = useCallback((line: string, accent: TelemetryEvent["accent"] = "blue") => {
    logCounterRef.current += 1;
    const ms = ((performance.now() % 99000) / 1000).toFixed(3).padStart(6, "0");
    setLogs((prev) => [...prev.slice(-5), { id: logCounterRef.current, ts: ms, line, accent }]);
  }, []);

  const flashKey = useCallback((keyLabel: string) => {
    setActiveKeyFlash(keyLabel);
    window.setTimeout(() => {
      setActiveKeyFlash((cur) => (cur === keyLabel ? null : cur));
    }, 320);
  }, []);

  const triggerRippleAndDesktopAction = useCallback(
    (xPct: number, yPct: number, button: "LMB" | "MMB" | "RMB", shouldClick: boolean) => {
      const px = Math.round((xPct / 100) * VIRTUAL_W);
      const py = Math.round((yPct / 100) * VIRTUAL_H);
      pushLog(`-> zwlr_virtual_pointer_v1.motion_absolute(${px}, ${py}, ${VIRTUAL_W}, ${VIRTUAL_H})`, "blue");

      const hit = hitTestTarget(xPct, yPct);
      if (shouldClick) {
        const btnCode = button === "LMB" ? 272 : button === "MMB" ? 274 : 273;
        pushLog(`-> zwlr_virtual_pointer_v1.button(${btnCode} /* ${button} */, pressed -> released)`, "pink");
        const rippleId = Date.now() + Math.random();
        setRipples((prev) => [...prev.slice(-3), { id: rippleId, x: xPct, y: yPct, button }]);
        window.setTimeout(() => {
          setRipples((prev) => prev.filter((r) => r.id !== rippleId));
        }, 700);

        if (hit) {
          setActiveTarget(`${button} @ ${hit.name} → ${hit.action}`);
          if (hit.id === "ws-1") setActiveWs(1);
          if (hit.id === "ws-2") setActiveWs(2);
          if (hit.id === "ws-3") setActiveWs(3);
          if (hit.id === "file-main") setActiveEditorFile("src/main.rs");
          if (hit.id === "file-hint") setActiveEditorFile("src/hint.rs");
          if (hit.id === "file-detect") setActiveEditorFile("src/target_detection.rs");
          if (hit.id === "btn-star") setStarred((s) => !s);
        } else {
          setActiveTarget(`${button} emitted at (${px}, ${py})`);
        }
      } else {
        setActiveTarget(
          hit
            ? `Warped to ${hit.name} (${px}, ${py}) — click_after_warp = false`
            : `Warped cursor to (${px}, ${py})`
        );
      }
    },
    [pushLog]
  );

  const staticHints = useMemo(() => generateStaticHints(config.hint_chars, 9, 5), [config.hint_chars]);
  const detectHints = useMemo(() => generateDetectHints(config.hint_chars), [config.hint_chars]);
  const activeHints = mode === "hint-static" ? staticHints : detectHints;

  const clearAutoTimers = useCallback(() => {
    autoTimersRef.current.forEach((id) => window.clearTimeout(id));
    autoTimersRef.current = [];
    setAutoPlaying(false);
    heldDirsRef.current.clear();
    shiftRef.current = false;
    setShiftHeld(false);
  }, []);

  const resetModeState = useCallback(
    (nextMode: DemoMode) => {
      clearAutoTimers();
      setMode(nextMode);
      setTyped("");
      setGridSel({ x: 0, y: 0, w: 100, h: 100, depth: 0 });
      if (nextMode === "hint-detect") {
        setDetectStage("hints");
        pushLog("mode -> --hint (hint_source=detect, 24 detected targets)", "mauve");
      } else if (nextMode === "hint-static") {
        pushLog(`mode -> --hint (hint_source=static, ${staticHints.length} cells)`, "mauve");
      } else if (nextMode === "grid") {
        pushLog("mode -> --grid (2x2 recursive subdivision)", "mauve");
      } else {
        pushLog("mode -> --normal (120fps Cairo crosshair)", "mauve");
      }
    },
    [clearAutoTimers, pushLog, staticHints.length]
  );

  const handleHintChar = useCallback(
    (ch: string) => {
      const lower = ch.toLowerCase();
      if (!/^[a-z]$/.test(lower)) return;
      flashKey(lower.toUpperCase());
      setTyped((prev) => {
        const candidate = prev + lower;
        const matches = activeHints.filter((h) => h.label.startsWith(candidate));
        if (matches.length === 0) {
          pushLog(`filter "${candidate}" -> 0 matches (ignored)`, "pink");
          return prev;
        }
        if (matches.length === 1) {
          const hit = matches[0];
          setCursor({ x: hit.cx, y: hit.cy });
          pushLog(`hint "${hit.label}" selected -> warping`, "green");
          triggerRippleAndDesktopAction(hit.cx, hit.cy, "LMB", config.click_after_warp);
          return "";
        }
        pushLog(`filter "${candidate}" -> ${matches.length} hints remaining`, "blue");
        return candidate;
      });
    },
    [activeHints, config.click_after_warp, flashKey, pushLog, triggerRippleAndDesktopAction]
  );

  const handleGridQuadrant = useCallback(
    (quadIdx: 0 | 1 | 2 | 3) => {
      const keyName = config.grid_quadrant_keys[quadIdx]?.toUpperCase() || "?";
      flashKey(keyName);
      setGridSel((prev) => {
        const next = subdivideGrid(prev, quadIdx);
        const pxW = Math.round((next.w / 100) * VIRTUAL_W);
        const pxH = Math.round((next.h / 100) * VIRTUAL_H);
        const cx = next.x + next.w / 2;
        const cy = next.y + next.h / 2;
        setCursor({ x: cx, y: cy });

        if (pxW < config.grid_min_size && pxH < config.grid_min_size) {
          pushLog(`grid cell ${pxW}x${pxH} < grid_min_size (${config.grid_min_size}) -> auto-warp`, "green");
          triggerRippleAndDesktopAction(cx, cy, "LMB", config.click_after_warp);
          return { x: 0, y: 0, w: 100, h: 100, depth: 0 };
        }
        pushLog(`grid subdivide Q${quadIdx + 1} (${keyName}) -> ${pxW}x${pxH}px`, "blue");
        return next;
      });
    },
    [config.click_after_warp, config.grid_min_size, config.grid_quadrant_keys, flashKey, pushLog, triggerRippleAndDesktopAction]
  );

  const handleGridClickCenter = useCallback(() => {
    flashKey(config.buttons[0].toUpperCase());
    const cx = gridSel.x + gridSel.w / 2;
    const cy = gridSel.y + gridSel.h / 2;
    setCursor({ x: cx, y: cy });
    triggerRippleAndDesktopAction(cx, cy, "LMB", true);
    setGridSel({ x: 0, y: 0, w: 100, h: 100, depth: 0 });
  }, [config.buttons, flashKey, gridSel, triggerRippleAndDesktopAction]);

  // Smooth RAF movement loop for Normal Mode
  useEffect(() => {
    if (mode !== "normal") return;
    let rafId = 0;
    let lastTs = performance.now();

    const tick = (now: number) => {
      const dt = Math.min((now - lastTs) / 1000, 0.05);
      lastTs = now;
      const dirs = heldDirsRef.current;
      if (dirs.size > 0) {
        const basePctPerSec = (config.speed / VIRTUAL_W) * 100 * 1.45;
        const mult = shiftRef.current ? Math.min(config.speed_modifier_multiplier, 6) : 1;
        const stepX = basePctPerSec * mult * dt;
        const stepY = stepX * (16 / 9);
        setCursor((prev) => {
          let nx = prev.x;
          let ny = prev.y;
          if (dirs.has("left")) nx -= stepX;
          if (dirs.has("right")) nx += stepX;
          if (dirs.has("up")) ny -= stepY;
          if (dirs.has("down")) ny += stepY;
          return {
            x: Math.max(1, Math.min(99, nx)),
            y: Math.max(1, Math.min(99, ny)),
          };
        });
      }
      rafId = window.requestAnimationFrame(tick);
    };

    rafId = window.requestAnimationFrame(tick);
    return () => window.cancelAnimationFrame(rafId);
  }, [config.speed, config.speed_modifier_multiplier, mode]);

  // Keyboard listener when playground is focused
  useEffect(() => {
    if (!focused) return;

    const onKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;

      const keyLower = e.key.toLowerCase();

      if (e.key === "Escape") {
        e.preventDefault();
        flashKey("ESC");
        setTyped("");
        setGridSel({ x: 0, y: 0, w: 100, h: 100, depth: 0 });
        pushLog("Escape -> overlay cancelled / reset", "pink");
        return;
      }

      if (mode === "hint-static" || mode === "hint-detect") {
        if (mode === "hint-detect" && detectStage !== "hints") {
          setDetectStage("hints");
        }
        if (e.key === "Backspace") {
          e.preventDefault();
          flashKey("BKSP");
          setTyped((p) => p.slice(0, -1));
          return;
        }
        if (/^[a-z]$/.test(keyLower)) {
          e.preventDefault();
          handleHintChar(keyLower);
        }
        return;
      }

      if (mode === "grid") {
        const qIdx = config.grid_quadrant_keys.findIndex((k) => k.toLowerCase() === keyLower);
        if (qIdx !== -1) {
          e.preventDefault();
          handleGridQuadrant(qIdx as 0 | 1 | 2 | 3);
          return;
        }
        if (keyLower === config.buttons[0].toLowerCase()) {
          e.preventDefault();
          handleGridClickCenter();
        }
        return;
      }

      if (mode === "normal") {
        if (e.key === "Shift") {
          shiftRef.current = true;
          setShiftHeld(true);
          return;
        }
        const [leftK, downK, upK, rightK] = config.normal_move_keys.map((k) => k.toLowerCase());
        if (keyLower === leftK) {
          e.preventDefault();
          heldDirsRef.current.add("left");
          flashKey(leftK.toUpperCase());
          return;
        }
        if (keyLower === downK) {
          e.preventDefault();
          heldDirsRef.current.add("down");
          flashKey(downK.toUpperCase());
          return;
        }
        if (keyLower === upK) {
          e.preventDefault();
          heldDirsRef.current.add("up");
          flashKey(upK.toUpperCase());
          return;
        }
        if (keyLower === rightK) {
          e.preventDefault();
          heldDirsRef.current.add("right");
          flashKey(rightK.toUpperCase());
          return;
        }

        const [lmb, mmb, rmb] = config.buttons.map((k) => k.toLowerCase());
        if (keyLower === lmb) {
          e.preventDefault();
          flashKey(lmb.toUpperCase());
          triggerRippleAndDesktopAction(cursorRef.current.x, cursorRef.current.y, "LMB", true);
          return;
        }
        if (keyLower === mmb) {
          e.preventDefault();
          flashKey(mmb.toUpperCase());
          triggerRippleAndDesktopAction(cursorRef.current.x, cursorRef.current.y, "MMB", true);
          return;
        }
        if (keyLower === rmb) {
          e.preventDefault();
          flashKey(rmb.toUpperCase());
          triggerRippleAndDesktopAction(cursorRef.current.x, cursorRef.current.y, "RMB", true);
          return;
        }
        if (keyLower === "x") {
          e.preventDefault();
          flashKey("X");
          resetModeState("hint-static");
          return;
        }
        if (keyLower === "g") {
          e.preventDefault();
          flashKey("G");
          resetModeState("grid");
        }
      }
    };

    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === "Shift") {
        shiftRef.current = false;
        setShiftHeld(false);
      }
      const keyLower = e.key.toLowerCase();
      const [leftK, downK, upK, rightK] = config.normal_move_keys.map((k) => k.toLowerCase());
      if (keyLower === leftK) heldDirsRef.current.delete("left");
      if (keyLower === downK) heldDirsRef.current.delete("down");
      if (keyLower === upK) heldDirsRef.current.delete("up");
      if (keyLower === rightK) heldDirsRef.current.delete("right");
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, [
    config.buttons,
    config.grid_quadrant_keys,
    config.normal_move_keys,
    detectStage,
    flashKey,
    focused,
    handleGridClickCenter,
    handleGridQuadrant,
    handleHintChar,
    mode,
    pushLog,
    resetModeState,
    triggerRippleAndDesktopAction,
  ]);

  // Guided choreographed Auto-Demo for the active mode
  const runAutoDemo = useCallback(() => {
    clearAutoTimers();
    setAutoPlaying(true);
    setTyped("");
    setGridSel({ x: 0, y: 0, w: 100, h: 100, depth: 0 });

    const schedule = (ms: number, fn: () => void) => {
      const id = window.setTimeout(fn, ms);
      autoTimersRef.current.push(id);
    };

    if (mode === "hint-detect") {
      const targetHint = detectHints.find((h) => h.id === "btn-star") || detectHints[0];
      const secondHint = detectHints.find((h) => h.id === "btn-run-test") || detectHints[1];
      setDetectStage("screencopy");
      pushLog("auto-demo: zwlr_screencopy_manager_v1 capture frame", "mauve");
      schedule(700, () => {
        setDetectStage("edges");
        pushLog("auto-demo: Canny(50, 160) + morph dilate(4x3)", "blue");
      });
      schedule(1500, () => {
        setDetectStage("filter");
        pushLog("auto-demo: contour hierarchy filter -> 24 kept, 5 pruned", "green");
      });
      schedule(2300, () => {
        setDetectStage("hints");
        pushLog(`auto-demo: typing "${targetHint.label}" to hit ${targetHint.targetName}`, "mauve");
      });
      targetHint.label.split("").forEach((ch, idx) => {
        schedule(3000 + idx * 550, () => handleHintChar(ch));
      });
      const offset = 3000 + targetHint.label.length * 550 + 900;
      secondHint.label.split("").forEach((ch, idx) => {
        schedule(offset + idx * 550, () => handleHintChar(ch));
      });
      schedule(offset + secondHint.label.length * 550 + 700, () => setAutoPlaying(false));
      return;
    }

    if (mode === "hint-static") {
      const pickA = staticHints[14] || staticHints[0];
      const pickB = staticHints[31] || staticHints[1];
      pushLog(`auto-demo: filtering static grid with "${pickA.label}"`, "mauve");
      pickA.label.split("").forEach((ch, idx) => {
        schedule(450 + idx * 580, () => handleHintChar(ch));
      });
      const nextStart = 450 + pickA.label.length * 580 + 1000;
      pickB.label.split("").forEach((ch, idx) => {
        schedule(nextStart + idx * 580, () => handleHintChar(ch));
      });
      schedule(nextStart + pickB.label.length * 580 + 700, () => setAutoPlaying(false));
      return;
    }

    if (mode === "grid") {
      const seq: Array<0 | 1 | 2 | 3> = [1, 0, 3, 2];
      pushLog("auto-demo: recursive quadrant drill-down (i -> u -> k -> j)", "mauve");
      seq.forEach((q, idx) => {
        schedule(550 + idx * 650, () => handleGridQuadrant(q));
      });
      schedule(550 + seq.length * 650 + 500, () => setAutoPlaying(false));
      return;
    }

    // Normal mode demo
    setCursor({ x: 50, y: 50 });
    pushLog("auto-demo: normal mode Shift+h/k turbo glide -> m click", "mauve");
    schedule(250, () => {
      shiftRef.current = true;
      setShiftHeld(true);
      heldDirsRef.current.add("left");
      heldDirsRef.current.add("up");
      flashKey("SHIFT + H/K");
    });
    schedule(900, () => {
      shiftRef.current = false;
      setShiftHeld(false);
      heldDirsRef.current.clear();
      heldDirsRef.current.add("right");
      flashKey("L");
    });
    schedule(1400, () => {
      heldDirsRef.current.clear();
      flashKey(config.buttons[0].toUpperCase());
      triggerRippleAndDesktopAction(cursorRef.current.x, cursorRef.current.y, "LMB", true);
    });
    schedule(2000, () => setAutoPlaying(false));
  }, [
    clearAutoTimers,
    config.buttons,
    detectHints,
    flashKey,
    handleGridQuadrant,
    handleHintChar,
    mode,
    pushLog,
    staticHints,
    triggerRippleAndDesktopAction,
  ]);

  useEffect(() => clearAutoTimers, [clearAutoTimers]);

  const currentModeInfo = MODE_TABS.find((m) => m.id === mode)!;
  const gridPxW = Math.round((gridSel.w / 100) * VIRTUAL_W);
  const gridPxH = Math.round((gridSel.h / 100) * VIRTUAL_H);

  return (
    <section id="playground" className="section playground-section">
      <div className="section-head">
        <div>
          <span className="eyebrow">Interactive Wayland Sandbox</span>
          <h2>Experience Every Mode Live</h2>
          <p className="section-sub">
            Click <strong>Auto-Play</strong> for a guided demo, or click inside the compositor frame and
            use your real keyboard — authentic <code>warpd-rs</code> label generation, quadrant math, and
            live Wayland protocol telemetry.
          </p>
        </div>
        <div className="playground-actions">
          <button
            type="button"
            className={`btn ${autoPlaying ? "btn-danger" : "btn-primary"}`}
            onClick={autoPlaying ? clearAutoTimers : runAutoDemo}
          >
            {autoPlaying ? <Square size={15} /> : <Play size={15} />}
            {autoPlaying ? "Stop Demo" : "Auto-Play Demo"}
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => resetModeState(mode)}
            title="Reset current mode"
          >
            <RotateCcw size={15} />
            Reset
          </button>
        </div>
      </div>

      {/* Mode Selector Bar */}
      <div className="mode-tabs" role="tablist" aria-label="warpd-rs modes">
        {MODE_TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={mode === tab.id}
            className={`mode-tab ${mode === tab.id ? "active" : ""}`}
            onClick={() => resetModeState(tab.id)}
          >
            <span className="mode-tab-top">
              {tab.icon}
              <span>{tab.label}</span>
            </span>
            <span className="mode-tab-cmd">{tab.cmd}</span>
          </button>
        ))}
      </div>

      {/* OpenCV Pipeline Stepper (visible in Detect Mode) */}
      {mode === "hint-detect" && (
        <div className="detect-pipeline-bar">
          <span className="pipeline-label">
            <Sparkles size={14} /> OpenCV Pipeline Stage:
          </span>
          <div className="pipeline-steps">
            {DETECT_STAGES.map((st) => (
              <button
                key={st.id}
                type="button"
                className={`pipeline-step ${detectStage === st.id ? "active" : ""}`}
                onClick={() => {
                  setDetectStage(st.id);
                  pushLog(`opencv stage -> ${st.step}_${st.id}`, "mauve");
                }}
              >
                <span className="step-num">{st.step}</span>
                <span>{st.label}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Simulated Wayland Desktop Compositor */}
      <div
        ref={stageRef}
        tabIndex={0}
        className={`wayland-stage ${focused ? "is-focused" : ""} ${
          mode === "hint-detect" && detectStage === "edges" ? "stage-edges-filter" : ""
        }`}
        onClick={() => {
          setFocused(true);
          stageRef.current?.focus();
        }}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        aria-label="Simulated Wayland compositor playground. Click and type keys to control warpd-rs."
      >
        {/* Waybar */}
        <div className="mock-waybar">
          <div className="mock-ws-group">
            {[1, 2, 3].map((n) => (
              <span key={n} className={`mock-ws ${activeWs === n ? "active" : ""}`}>
                {n}
              </span>
            ))}
            <span className="mock-bar-title">Hyprland · HDMI-A-2 (2560×1440)</span>
          </div>
          <div className="mock-bar-right">
            <span className="mock-bar-pill">wlroots-layer-shell</span>
            <span className="mock-bar-pill">Wi-Fi 1.2G</span>
            <span className="mock-bar-pill">12:42</span>
          </div>
        </div>

        {/* Tiled Desktop Panes */}
        <div className="mock-grid">
          {/* Left Pane: Browser */}
          <div className="mock-window mock-browser">
            <div className="mock-tabs">
              <div className="mock-tab active">warpd.jesse.rs</div>
              <div className="mock-tab">jesseconis/warpd-rs</div>
            </div>
            <div className="mock-urlbar">
              <span className="mock-icon-btn">↻</span>
              <span className="mock-url-input">https://warpd.jesse.rs/docs/</span>
              <span className={`mock-star-btn ${starred ? "starred" : ""}`}>
                {starred ? "★ Starred" : "☆ Star"}
              </span>
            </div>
            <div className="mock-browser-body">
              <div className="mock-hero-card">
                <div className="mock-badge">Rust · Wayland · Zero Daemon</div>
                <div className="mock-h">warpd-rs virtual pointer</div>
                <div className="mock-p">Instant keyboard precision across multi-monitor wlroots setups.</div>
              </div>
              <div className="mock-pill-row">
                <span className="mock-ui-chip">Hint Mode</span>
                <span className="mock-ui-chip">Grid Mode</span>
                <span className="mock-ui-chip">Normal Mode</span>
              </div>
              <div className="mock-cta-row">
                <span className="mock-ui-btn primary">Download v0.2.2</span>
                <span className="mock-ui-btn">Read Docs</span>
              </div>
              <div className="mock-divider" />
              <div className="mock-cta-row">
                <span className="mock-ui-btn subtle">OpenCV Detect: ON</span>
                <span className="mock-ui-btn subtle">Copy config.toml</span>
              </div>
            </div>
          </div>

          {/* Right Column: Editor + Terminal */}
          <div className="mock-right-col">
            <div className="mock-window mock-editor">
              <div className="mock-win-header">
                <span className="mock-win-title">nvim — {activeEditorFile}</span>
                <span className="mock-ui-btn tiny">▶ cargo test</span>
              </div>
              <div className="mock-editor-split">
                <div className="mock-tree">
                  <div className={`mock-file ${activeEditorFile === "src/main.rs" ? "active" : ""}`}>
                    src/main.rs
                  </div>
                  <div className={`mock-file ${activeEditorFile === "src/hint.rs" ? "active" : ""}`}>
                    src/hint.rs
                  </div>
                  <div
                    className={`mock-file ${
                      activeEditorFile === "src/target_detection.rs" ? "active" : ""
                    }`}
                  >
                    target_detection.rs
                  </div>
                </div>
                <pre className="mock-code">
{`pub fn generate_labels(chars: &str, count: usize) -> Vec<String> {
  // uniform prefix-free radix encoding
  while capacity < count {
    width += 1;
    capacity = capacity.saturating_mul(radix);
  }
}`}
                </pre>
              </div>
            </div>

            <div className="mock-window mock-terminal">
              <div className="mock-win-header">
                <span className="mock-win-title">kitty — zsh</span>
                <span className="mock-win-meta">WAYLAND_DEBUG=1</span>
              </div>
              <div className="mock-term-body">
                <div>
                  <span className="t-prompt">❯</span> {currentModeInfo.cmd}
                </div>
                <div className="t-dim">info: bound zwlr_virtual_pointer_v1 on HDMI-A-2</div>
                <div className="mock-term-actions">
                  <span className="mock-ui-btn tiny">--hint</span>
                  <span className="mock-ui-btn tiny">--grid</span>
                  <span className="mock-ui-btn tiny">clear</span>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* === OVERLAY LAYER: OpenCV Prune/Keep visualization === */}
        {mode === "hint-detect" && (detectStage === "filter" || detectStage === "edges") && (
          <div className="overlay-layer detect-viz-layer" aria-hidden="true">
            {detectStage === "filter" &&
              PRUNED_CONTAINERS.map((box) => (
                <div
                  key={box.id}
                  className="detect-rect pruned"
                  style={{
                    left: `${box.x}%`,
                    top: `${box.y}%`,
                    width: `${box.w}%`,
                    height: `${box.h}%`,
                  }}
                />
              ))}
            {DESKTOP_TARGETS.map((box) => (
              <div
                key={box.id}
                className={`detect-rect ${detectStage === "edges" ? "edge-box" : "kept"}`}
                style={{
                  left: `${box.x}%`,
                  top: `${box.y}%`,
                  width: `${box.w}%`,
                  height: `${box.h}%`,
                }}
              />
            ))}
            <div className="detect-legend">
              {detectStage === "edges" ? (
                <span>Canny(50, 160) + 4×3 morphological dilation</span>
              ) : (
                <>
                  <span className="legend-dot kept" /> Forwarded UI Targets (24)
                  <span className="legend-dot pruned" /> Pruned Containers (5)
                </>
              )}
            </div>
          </div>
        )}

        {/* === OVERLAY LAYER: Hint Boxes (Static & Detect) === */}
        {(mode === "hint-static" || (mode === "hint-detect" && detectStage === "hints")) && (
          <div className="overlay-layer hint-overlay-layer">
            {activeHints.map((h) => {
              const matches = !typed || h.label.startsWith(typed);
              if (!matches) return null;
              const prefix = typed;
              const rest = h.label.slice(typed.length);
              return (
                <button
                  key={h.id}
                  type="button"
                  className={`hint-badge ${mode === "hint-detect" ? "detect-badge" : ""}`}
                  style={{
                    left: `${h.cx}%`,
                    top: `${h.cy}%`,
                    backgroundColor: config.hint_bgcolor,
                    opacity: config.hint_bg_opacity,
                    color: config.hint_fgcolor,
                    borderRadius: `${config.hint_border_radius}px`,
                  }}
                  onClick={(e) => {
                    e.stopPropagation();
                    setFocused(true);
                    setTyped("");
                    setCursor({ x: h.cx, y: h.cy });
                    pushLog(`hint "${h.label}" clicked -> warp`, "green");
                    triggerRippleAndDesktopAction(h.cx, h.cy, "LMB", config.click_after_warp);
                  }}
                  title={h.targetName ? `${h.targetName} (type "${h.label}")` : `Type "${h.label}"`}
                >
                  {prefix && <span className="hint-prefix">{prefix}</span>}
                  <span>{rest}</span>
                </button>
              );
            })}
          </div>
        )}

        {/* === OVERLAY LAYER: Grid Mode === */}
        {mode === "grid" && (
          <div className="overlay-layer grid-overlay-layer">
            <div
              className="grid-box"
              style={{
                left: `${gridSel.x}%`,
                top: `${gridSel.y}%`,
                width: `${gridSel.w}%`,
                height: `${gridSel.h}%`,
                borderColor: config.grid_color,
                borderWidth: `${config.grid_border_size}px`,
              }}
            >
              <div
                className="grid-line-v"
                style={{ backgroundColor: config.grid_color, width: `${config.grid_border_size}px` }}
              />
              <div
                className="grid-line-h"
                style={{ backgroundColor: config.grid_color, height: `${config.grid_border_size}px` }}
              />
              {[0, 1, 2, 3].map((q) => (
                <button
                  key={q}
                  type="button"
                  className={`grid-quad q-${q}`}
                  style={{ color: config.grid_color }}
                  onClick={(e) => {
                    e.stopPropagation();
                    setFocused(true);
                    handleGridQuadrant(q as 0 | 1 | 2 | 3);
                  }}
                >
                  <span>{config.grid_quadrant_keys[q]}</span>
                </button>
              ))}
            </div>
            <div className="grid-hud">
              Cell: <strong>{gridPxW}×{gridPxH}px</strong> · auto-warp &lt; {config.grid_min_size}px · press{" "}
              <kbd>{config.buttons[0]}</kbd> to click centre
            </div>
          </div>
        )}

        {/* === OVERLAY LAYER: Normal Mode Crosshair === */}
        {mode === "normal" && (
          <div className="overlay-layer normal-overlay-layer">
            <div
              className="crosshair-v"
              style={{
                left: `${cursor.x}%`,
                backgroundColor: config.cursor_color,
                width: `${config.crosshair_line_width}px`,
              }}
            />
            <div
              className="crosshair-h"
              style={{
                top: `${cursor.y}%`,
                backgroundColor: config.cursor_color,
                height: `${config.crosshair_line_width}px`,
              }}
            />
            <div
              className="normal-dot"
              style={{
                left: `${cursor.x}%`,
                top: `${cursor.y}%`,
                backgroundColor: config.cursor_color,
                width: `${config.cursor_size * 2}px`,
                height: `${config.cursor_size * 2}px`,
              }}
            />
            <div className="grid-hud">
              Pos: <strong>{Math.round((cursor.x / 100) * VIRTUAL_W)}, {Math.round((cursor.y / 100) * VIRTUAL_H)}</strong>{" "}
              · Speed: <strong>{shiftHeld ? `${config.speed_modifier_multiplier}× BOOST` : "1×"}</strong> ·{" "}
              <kbd>x</kbd> hint · <kbd>g</kbd> grid
            </div>
          </div>
        )}

        {/* Virtual Pointer Dot (in hint/grid modes) */}
        {mode !== "normal" && (
          <div
            className="virtual-pointer"
            style={{
              left: `${cursor.x}%`,
              top: `${cursor.y}%`,
              borderColor: config.cursor_color,
            }}
            aria-hidden="true"
          />
        )}

        {/* Click Ripples */}
        {ripples.map((r) => (
          <div
            key={r.id}
            className={`click-ripple ripple-${r.button.toLowerCase()}`}
            style={{ left: `${r.x}%`, top: `${r.y}%` }}
            aria-hidden="true"
          >
            <span>{r.button}</span>
          </div>
        ))}

        {/* Focus Prompt Pill */}
        <div className={`focus-indicator ${focused ? "active" : ""}`}>
          {focused
            ? typed
              ? `Typed prefix: "${typed}"`
              : "Keyboard Captured — Type keys directly"
            : "Click canvas to type with keyboard (or click hints/keycaps)"}
        </div>
      </div>

      {/* Tactile Keycap Controls + Telemetry Strip */}
      <div className="playground-footer">
        <div className="controls-card">
          <div className="controls-head">
            <span className="controls-title">{currentModeInfo.blurb}</span>
            {activeKeyFlash && <span className="key-flash">KEY: {activeKeyFlash}</span>}
          </div>

          <div className="keycap-strip">
            {(mode === "hint-static" || mode === "hint-detect") && (
              <>
                <span className="strip-label">Quick try:</span>
                {activeHints.slice(0, 6).map((h) => (
                  <button
                    key={h.id}
                    type="button"
                    className="keycap-btn"
                    onClick={() => {
                      if (mode === "hint-detect" && detectStage !== "hints") setDetectStage("hints");
                      h.label.split("").forEach((ch, idx) => {
                        window.setTimeout(() => handleHintChar(ch), idx * 140);
                      });
                    }}
                  >
                    <kbd>{h.label}</kbd>
                    {h.targetName && <span className="keycap-sub">{h.targetName}</span>}
                  </button>
                ))}
                <button
                  type="button"
                  className="keycap-btn"
                  onClick={() => setTyped((p) => p.slice(0, -1))}
                >
                  <kbd>Bksp</kbd>
                </button>
              </>
            )}

            {mode === "grid" && (
              <>
                <span className="strip-label">Subdivide:</span>
                {["TL", "TR", "BL", "BR"].map((pos, idx) => (
                  <button
                    key={pos}
                    type="button"
                    className="keycap-btn"
                    onClick={() => handleGridQuadrant(idx as 0 | 1 | 2 | 3)}
                  >
                    <kbd>{config.grid_quadrant_keys[idx]}</kbd>
                    <span className="keycap-sub">{pos}</span>
                  </button>
                ))}
                <button type="button" className="keycap-btn accent" onClick={handleGridClickCenter}>
                  <kbd>{config.buttons[0]}</kbd>
                  <span className="keycap-sub">Click Centre</span>
                </button>
              </>
            )}

            {mode === "normal" && (
              <>
                <span className="strip-label">Steer (hold):</span>
                {(
                  [
                    ["left", config.normal_move_keys[0], "←"],
                    ["down", config.normal_move_keys[1], "↓"],
                    ["up", config.normal_move_keys[2], "↑"],
                    ["right", config.normal_move_keys[3], "→"],
                  ] as const
                ).map(([dir, key, arrow]) => (
                  <button
                    key={dir}
                    type="button"
                    className="keycap-btn"
                    onPointerDown={() => {
                      heldDirsRef.current.add(dir);
                      flashKey(key.toUpperCase());
                    }}
                    onPointerUp={() => heldDirsRef.current.delete(dir)}
                    onPointerLeave={() => heldDirsRef.current.delete(dir)}
                  >
                    <kbd>{key}</kbd>
                    <span className="keycap-sub">{arrow}</span>
                  </button>
                ))}
                <button
                  type="button"
                  className={`keycap-btn ${shiftHeld ? "accent" : ""}`}
                  onClick={() => {
                    shiftRef.current = !shiftRef.current;
                    setShiftHeld(shiftRef.current);
                  }}
                >
                  <kbd>Shift</kbd>
                  <span className="keycap-sub">{config.speed_modifier_multiplier}×</span>
                </button>
                <button
                  type="button"
                  className="keycap-btn accent"
                  onClick={() =>
                    triggerRippleAndDesktopAction(cursorRef.current.x, cursorRef.current.y, "LMB", true)
                  }
                >
                  <kbd>{config.buttons[0]}</kbd>
                  <span className="keycap-sub">LMB</span>
                </button>
                <button
                  type="button"
                  className="keycap-btn"
                  onClick={() =>
                    triggerRippleAndDesktopAction(cursorRef.current.x, cursorRef.current.y, "RMB", true)
                  }
                >
                  <kbd>{config.buttons[2]}</kbd>
                  <span className="keycap-sub">RMB</span>
                </button>
              </>
            )}
          </div>

          <div className="status-banner">{activeTarget}</div>
        </div>

        {/* Live Wayland Protocol Trace */}
        <div className="telemetry-card">
          <div className="telemetry-head">
            <Terminal size={14} />
            <span>WAYLAND_DEBUG Stream</span>
          </div>
          <div className="telemetry-lines">
            {logs.map((entry) => (
              <div key={entry.id} className={`telemetry-line accent-${entry.accent || "blue"}`}>
                <span className="t-ts">[{entry.ts}]</span>
                <span>{entry.line}</span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
};
