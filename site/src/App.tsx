import React, { useState } from "react";
import {
  ArrowRight,
  BookOpen,
  Check,
  Copy,
  Cpu,
  ExternalLink,
  Github,
  Layers,
  ScanEye,
  Terminal,
  Zap,
} from "lucide-react";
import { REPO_META, EXTRA_DOC_PAGES } from "./generated/repoMeta";
import { INITIAL_CONFIG, WarpdConfig } from "./lib/warpdEngine";
import { InteractivePlayground } from "./components/InteractivePlayground";
import { RealCaptures } from "./components/RealCaptures";
import { ConfigLab } from "./components/ConfigLab";

const CLI_EXAMPLES = [
  {
    id: "hint",
    label: "--hint",
    cmd: "warpd-rs --hint",
    note: "Overlay prefix-free hint labels (static grid, OpenCV detect, or stdin rectangles).",
  },
  {
    id: "grid",
    label: "--grid",
    cmd: "warpd-rs --grid",
    note: "Recursively subdivide the focused output into 2×2 quadrants with u / i / j / k.",
  },
  {
    id: "normal",
    label: "--normal",
    cmd: "warpd-rs --normal",
    note: "Continuous 120fps h/j/k/l pointer steering with Shift boost and m / , / . clicks.",
  },
  {
    id: "config",
    label: "--generate-config",
    cmd: "warpd-rs --generate-config",
    note: "Write a ready-to-edit default TOML config to ~/.config/warpd-rs/config.toml.",
  },
];

export const App: React.FC = () => {
  const [config, setConfig] = useState<WarpdConfig>(INITIAL_CONFIG);
  const [cliIdx, setCliIdx] = useState(0);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const copyText = async (id: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedId(id);
      window.setTimeout(() => setCopiedId((cur) => (cur === id ? null : cur)), 1400);
    } catch {
      // ignore clipboard errors
    }
  };

  const activeCli = CLI_EXAMPLES[cliIdx];

  return (
    <div className="app-shell">
      {/* Sticky Header */}
      <header className="site-header">
        <div className="container header-inner">
          <a href="/" className="brand">
            <img src="/favicon.svg" width={26} height={26} alt="" />
            <span className="brand-wordmark">{REPO_META.name}</span>
            <span className="version-badge">v{REPO_META.version}</span>
          </a>

          <nav className="top-nav" aria-label="Primary">
            <a href="#playground">Playground</a>
            <a href="#captures">Recordings</a>
            <a href="#config-lab">Config Lab</a>
            <a href="#install">Build</a>
            <a href="/docs/" className="nav-pill">
              <BookOpen size={14} /> Docs
            </a>
            <a
              href={REPO_META.repository}
              target="_blank"
              rel="noopener noreferrer"
              className="nav-pill nav-gh"
            >
              <Github size={14} /> GitHub
            </a>
          </nav>
        </div>
      </header>

      <main>
        {/* Hero Section */}
        <section className="hero">
          <div className="hero-glow" aria-hidden="true" />
          <div className="container hero-grid">
            <div className="hero-copy">
              <div className="hero-kicker">
                <span className="kicker-dot" />
                <span>Wayland-First · Written in Rust · MIT Licensed</span>
              </div>
              <h1>
                Modal pointer control,
                <br />
                <span className="gradient-text">zero hand-off from home row.</span>
              </h1>
              <p className="hero-lead">
                <strong>warpd-rs</strong> is a fast, keyboard-driven virtual pointer for wlroots
                Wayland compositors (Hyprland, Sway, and friends). Warp anywhere with prefix-free
                hint labels, OpenCV UI detection, recursive grids, or 120fps crosshair navigation.
              </p>

              <div className="hero-ctas">
                <a href="#playground" className="btn btn-primary btn-lg">
                  Try Interactive Sandbox <ArrowRight size={16} />
                </a>
                <a href="/docs/" className="btn btn-secondary btn-lg">
                  <BookOpen size={16} /> Read the Docs
                </a>
                <a
                  href={REPO_META.repository}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="btn btn-ghost btn-lg"
                >
                  <Github size={16} /> Repo <ExternalLink size={13} />
                </a>
              </div>
            </div>

            {/* Interactive CLI Preview Card */}
            <div className="hero-terminal">
              <div className="term-chrome">
                <span className="dot red" />
                <span className="dot yellow" />
                <span className="dot green" />
                <span className="term-title">wlroots · wayland-client</span>
              </div>
              <div className="cli-tabs">
                {CLI_EXAMPLES.map((item, idx) => (
                  <button
                    key={item.id}
                    type="button"
                    className={`cli-tab ${idx === cliIdx ? "active" : ""}`}
                    onClick={() => setCliIdx(idx)}
                  >
                    {item.label}
                  </button>
                ))}
              </div>
              <div className="term-cmd-row">
                <code>
                  <span className="t-prompt">$</span> {activeCli.cmd}
                </code>
                <button
                  type="button"
                  className="copy-pill"
                  onClick={() => copyText("hero-cli", activeCli.cmd)}
                >
                  {copiedId === "hero-cli" ? <Check size={14} /> : <Copy size={14} />}
                  {copiedId === "hero-cli" ? "Copied" : "Copy"}
                </button>
              </div>
              <p className="term-note">{activeCli.note}</p>
              <div className="term-protocols">
                <span>zwlr_layer_shell_v1</span>
                <span>zwlr_virtual_pointer_v1</span>
                <span>zwlr_screencopy_v1</span>
                <span>zxdg_output_v1</span>
              </div>
            </div>
          </div>

          {/* Feature Highlights Strip */}
          <div className="container feature-strip">
            <div className="feature-card">
              <Zap size={18} className="f-icon blue" />
              <h3>Zero-Daemon One-Shot</h3>
              <p>
                Bind directly to compositor hotkeys. Renders directly into Cairo SHM layer surfaces
                and exits immediately after warping.
              </p>
            </div>
            <div className="feature-card">
              <ScanEye size={18} className="f-icon mauve" />
              <h3>OpenCV UI Detection</h3>
              <p>
                Captures the focused monitor via screencopy and filters contour hierarchies to snap
                hints onto real buttons, tabs, and inputs.
              </p>
            </div>
            <div className="feature-card">
              <Layers size={18} className="f-icon pink" />
              <h3>Never Drops Dense Targets</h3>
              <p>
                Uniform-width prefix-free radix labels automatically scale from 1 to 2, 3, or more
                chars on ultra-wide or dense screens.
              </p>
            </div>
            <div className="feature-card">
              <Cpu size={18} className="f-icon green" />
              <h3>Optional No-OpenCV Build</h3>
              <p>
                Want a minimal binary? Compile with <code>--no-default-features</code> to drop
                OpenCV while keeping static hints, stdin, grid, and normal modes.
              </p>
            </div>
          </div>
        </section>

        {/* Interactive Playground */}
        <div className="container">
          <InteractivePlayground config={config} />
          <RealCaptures />
          <ConfigLab config={config} onChange={setConfig} />

          {/* Build & Docs Section */}
          <section id="install" className="section">
            <div className="section-head">
              <div>
                <span className="eyebrow">Build From Source</span>
                <h2>Up and Running in Seconds</h2>
                <p className="section-sub">
                  Requires Rust {REPO_META.rustVersion}+. Default build links OpenCV 4.x or 5.x for{" "}
                  <code>hint_source = "detect"</code>.
                </p>
              </div>
            </div>

            <div className="install-grid">
              <div className="install-card">
                <div className="install-head">
                  <Terminal size={16} />
                  <span>Full Build (with OpenCV Detect)</span>
                  <button
                    type="button"
                    className="copy-pill"
                    onClick={() =>
                      copyText(
                        "build-full",
                        "git clone https://github.com/jesseconis/warpd-rs.git\ncd warpd-rs\ncargo build --release"
                      )
                    }
                  >
                    {copiedId === "build-full" ? <Check size={13} /> : <Copy size={13} />}
                    {copiedId === "build-full" ? "Copied" : "Copy"}
                  </button>
                </div>
                <pre>{`git clone https://github.com/jesseconis/warpd-rs.git\ncd warpd-rs\ncargo build --release`}</pre>
              </div>

              <div className="install-card">
                <div className="install-head">
                  <Terminal size={16} />
                  <span>Minimal Build (No OpenCV Dep)</span>
                  <button
                    type="button"
                    className="copy-pill"
                    onClick={() =>
                      copyText("build-nocv", "cargo build --release --no-default-features")
                    }
                  >
                    {copiedId === "build-nocv" ? <Check size={13} /> : <Copy size={13} />}
                    {copiedId === "build-nocv" ? "Copied" : "Copy"}
                  </button>
                </div>
                <pre>{`cargo build --release --no-default-features`}</pre>
                <p className="install-hint">
                  Or grab pre-built <code>warpd-rs</code> and <code>warpd-rs-nocv</code> binaries
                  from{" "}
                  <a
                    href={`${REPO_META.repository}/releases`}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    GitHub Releases (v{REPO_META.version}) &#8599;
                  </a>
                  .
                </p>
              </div>
            </div>

            {/* Docs Quick Links Banner */}
            <div className="docs-banner">
              <div>
                <h3>Explore the Static Documentation</h3>
                <p>
                  Generated automatically from the repository <code>README.md</code>,{" "}
                  <code>config.example.toml</code>, and <code>docs/</code> assets on every push.
                </p>
              </div>
              <div className="docs-banner-links">
                <a href="/docs/" className="btn btn-primary">
                  Overview (README) <ArrowRight size={15} />
                </a>
                <a href="/docs/configuration/" className="btn btn-secondary">
                  Config Reference
                </a>
                <a href="/docs/recordings/" className="btn btn-secondary">
                  Recordings &amp; OpenCV
                </a>
                {EXTRA_DOC_PAGES.map((doc) => (
                  <a key={doc.slug} href={doc.href} className="btn btn-ghost">
                    {doc.title}
                  </a>
                ))}
              </div>
            </div>
          </section>
        </div>
      </main>

      <footer className="site-footer">
        <div className="container footer-inner">
          <div className="footer-left">
            <img src="/favicon.svg" width={20} height={20} alt="" />
            <span>
              <strong>{REPO_META.name}</strong> v{REPO_META.version} · {REPO_META.license} License ·
              Inspired by{" "}
              <a href="https://github.com/rvaiya/warpd" target="_blank" rel="noopener noreferrer">
                warpd
              </a>
            </span>
          </div>
          <div className="footer-links">
            <a href="/docs/">Docs</a>
            <a href="/docs/configuration/">Configuration</a>
            <a href="/docs/recordings/">Recordings</a>
            <a href={REPO_META.repository} target="_blank" rel="noopener noreferrer">
              GitHub
            </a>
            <a
              href={`${REPO_META.repository}/releases`}
              target="_blank"
              rel="noopener noreferrer"
            >
              Releases
            </a>
          </div>
        </div>
      </footer>
    </div>
  );
};

export default App;
