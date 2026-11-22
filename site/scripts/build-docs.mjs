import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Marked } from "marked";
import hljs from "highlight.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SITE_DIR = path.resolve(__dirname, "..");
const REPO_ROOT = path.resolve(SITE_DIR, "..");

const args = process.argv.slice(2);
const outFlagIdx = args.indexOf("--out");
const outDirName = outFlagIdx !== -1 && args[outFlagIdx + 1] ? args[outFlagIdx + 1] : "dist";
const OUT_ROOT = path.resolve(SITE_DIR, outDirName);
const DOCS_OUT = path.join(OUT_ROOT, "docs");

function escapeHtml(str) {
  return String(str)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function slugify(text) {
  return text
    .toLowerCase()
    .replace(/<[^>]+>/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function stripHtml(html) {
  return html
    .replace(/<script[\s\S]*?<\/script>/gi, " ")
    .replace(/<style[\s\S]*?<\/style>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function parseCargoMeta() {
  const text = fs.readFileSync(path.join(REPO_ROOT, "Cargo.toml"), "utf8");
  const pkgMatch = text.match(/\[package\]([\s\S]*?)(?:\n\[|$)/);
  const pkg = pkgMatch ? pkgMatch[1] : text;
  const pick = (key, fallback = "") => {
    const m = pkg.match(new RegExp(`^${key}\\s*=\\s*"([^"]*)"`, "m"));
    return m ? m[1] : fallback;
  };
  return {
    name: pick("name", "warpd-rs"),
    version: pick("version", "0.2.2"),
    rustVersion: pick("rust-version", "1.88"),
    description: pick("description", "keyboard-driven virtual pointer for Wayland"),
    license: pick("license", "MIT"),
    repository: pick("repository", "https://github.com/jesseconis/warpd-rs"),
  };
}

function parseConfigToml(text) {
  const out = {};
  const lines = text.split(/\r?\n/);
  let i = 0;
  while (i < lines.length) {
    const line = lines[i].trim();
    i += 1;
    if (!line || line.startsWith("#")) continue;
    const eq = line.indexOf("=");
    if (eq === -1) continue;
    const key = line.slice(0, eq).trim();
    let val = line.slice(eq + 1).trim();
    if (val.startsWith("[") && !val.endsWith("]")) {
      while (i < lines.length && !val.includes("]")) {
        val += " " + lines[i].trim();
        i += 1;
      }
    }
    if (val.startsWith("[") && val.endsWith("]")) {
      out[key] = [...val.slice(1, -1).matchAll(/"([^"]*)"/g)].map((m) => m[1]);
    } else if (val === "true" || val === "false") {
      out[key] = val === "true";
    } else if (/^".*"$/.test(val)) {
      out[key] = val.slice(1, -1);
    } else if (/^-?\d+(\.\d+)?$/.test(val)) {
      out[key] = Number(val);
    } else {
      out[key] = val;
    }
  }
  return out;
}

function copyDocsAssets() {
  fs.mkdirSync(DOCS_OUT, { recursive: true });
  const srcDocsDir = path.join(REPO_ROOT, "docs");
  if (!fs.existsSync(srcDocsDir)) return { availableMp4s: new Set(), files: [] };

  const files = fs.readdirSync(srcDocsDir).filter((f) => fs.statSync(path.join(srcDocsDir, f)).isFile());
  const availableMp4s = new Set(
    files.filter((f) => f.toLowerCase().endsWith(".mp4")).map((f) => f.replace(/\.mp4$/i, ""))
  );

  for (const file of files) {
    const ext = path.extname(file).toLowerCase();
    if (ext === ".md") continue;
    // Skip redundant large GIF when a matching MP4 is present for web delivery
    if (ext === ".gif" && availableMp4s.has(file.replace(/\.gif$/i, ""))) {
      continue;
    }
    fs.copyFileSync(path.join(srcDocsDir, file), path.join(DOCS_OUT, file));
  }

  return { availableMp4s, files };
}

function createMarkdownRenderer(availableMp4s) {
  const toc = [];
  const usedIds = new Map();

  const uniqueId = (text) => {
    const base = slugify(text) || "section";
    const count = usedIds.get(base) || 0;
    usedIds.set(base, count + 1);
    return count === 0 ? base : `${base}-${count}`;
  };

  const marked = new Marked({
    gfm: true,
    breaks: false,
    renderer: {
      heading({ tokens, depth }) {
        const html = this.parser.parseInline(tokens);
        const plain = stripHtml(html);
        const id = uniqueId(plain);
        if (depth >= 2 && depth <= 3) {
          toc.push({ id, text: plain, depth });
        }
        return `<h${depth} id="${id}"><a class="heading-anchor" href="#${id}" aria-label="Link to ${escapeHtml(plain)}">#</a>${html}</h${depth}>\n`;
      },
      code({ text, lang }) {
        const cleanLang = (lang || "").trim().toLowerCase();
        const resolved = cleanLang === "txt" ? "plaintext" : cleanLang;
        let highlighted;
        if (resolved && hljs.getLanguage(resolved)) {
          highlighted = hljs.highlight(text, { language: resolved }).value;
        } else {
          highlighted = escapeHtml(text);
        }
        const label = cleanLang || "text";
        return `<div class="code-block"><div class="code-block-bar"><span class="code-lang">${escapeHtml(label)}</span><button class="copy-btn" type="button" data-copy>Copy</button></div><pre><code class="hljs language-${escapeHtml(label)}">${highlighted}</code></pre></div>\n`;
      },
      image({ href, title, text }) {
        return renderMediaTag(href, text || "", title || "", availableMp4s);
      },
      link({ href, title, tokens }) {
        let targetHref = href || "";
        if (targetHref === "./config.example.toml" || targetHref === "config.example.toml") {
          targetHref = "/docs/configuration/";
        }
        const label = this.parser.parseInline(tokens);
        const isExternal = /^https?:\/\//i.test(targetHref);
        const extra = isExternal ? ' target="_blank" rel="noopener noreferrer"' : "";
        const titleAttr = title ? ` title="${escapeHtml(title)}"` : "";
        return `<a href="${escapeHtml(targetHref)}"${titleAttr}${extra}>${label}</a>`;
      },
    },
  });

  const render = (rawMarkdown) => {
    toc.length = 0;
    usedIds.clear();

    // Upgrade inline HTML <img src="docs/foo.gif" .../> to looping <video> backed by .mp4 when available
    const preprocessed = rawMarkdown.replace(
      /<img\s+[^>]*src=["']([^"']+)["'][^>]*\/?>/gi,
      (full, src) => {
        const altMatch = full.match(/alt=["']([^"']*)["']/i);
        const alt = altMatch ? altMatch[1] : "";
        return renderMediaTag(src, alt, "", availableMp4s);
      }
    );

    const html = marked
      .parse(preprocessed)
      .replace(/<p>\s*(<figure class="doc-media">[\s\S]*?<\/figure>)\s*<\/p>/g, "$1");
    return { html, toc: [...toc] };
  };

  return { render };
}

function renderMediaTag(src, alt, title, availableMp4s) {
  const normalized = src.replace(/^\.?\//, "");
  const matchDocs = normalized.match(/^docs\/([^/]+)$/);
  const cleanAlt = alt && alt.trim().toLowerCase() !== "here" ? alt.trim() : "";
  if (matchDocs) {
    const filename = matchDocs[1];
    const ext = path.extname(filename).toLowerCase();
    const stem = filename.replace(/\.[^.]+$/, "");
    if (ext === ".gif" && availableMp4s.has(stem)) {
      return `<figure class="doc-media"><video autoplay loop muted playsinline controls preload="metadata" src="/docs/${escapeHtml(stem)}.mp4"></video><figcaption>${escapeHtml(cleanAlt || `${stem.replace(/_/g, " ")} recording`)}</figcaption></figure>`;
    }
    const caption = cleanAlt || filename;
    return `<figure class="doc-media"><img src="/docs/${escapeHtml(filename)}" alt="${escapeHtml(caption)}" loading="lazy" /><figcaption>${escapeHtml(caption)}</figcaption></figure>`;
  }
  const titleAttr = title ? ` title="${escapeHtml(title)}"` : "";
  return `<figure class="doc-media"><img src="${escapeHtml(src)}" alt="${escapeHtml(cleanAlt)}"${titleAttr} loading="lazy" /></figure>`;
}

const CONFIG_DESCRIPTIONS = {
  hint_source: {
    group: "Hint Mode",
    type: "string",
    desc: 'Target generation source: `"static"` (fixed grid), `"detect"` (OpenCV screencopy target detection), or `"stdin"` (`wxh+x+y` rectangles).',
  },
  hint_chars: {
    group: "Hint Mode",
    type: "string",
    desc: "Ordered ASCII alphabet used for prefix-free uniform-length hint labels. Falls back to `a–z` if fewer than two unique letters.",
  },
  hint_bgcolor: {
    group: "Hint Mode",
    type: "hex color",
    desc: "Background fill color for hint label badges (`#RRGGBB` or `#RRGGBBAA`).",
  },
  hint_bg_opacity: {
    group: "Hint Mode",
    type: "float",
    desc: "Opacity multiplier (`0.0`–`1.0`) applied to hint badge background.",
  },
  hint_fgcolor: {
    group: "Hint Mode",
    type: "hex color",
    desc: "Foreground text color for hint labels.",
  },
  hint_border_radius: {
    group: "Hint Mode",
    type: "float (px)",
    desc: "Corner rounding radius for hint badges in pixels.",
  },
  hint_font: {
    group: "Hint Mode",
    type: "string",
    desc: "Cairo font family used to render hint labels.",
  },
  hint_font_size: {
    group: "Hint Mode",
    type: "float (px)",
    desc: "Font size for hint labels in pixels.",
  },
  hint_grid_gap: {
    group: "Hint Mode",
    type: "integer (px)",
    desc: "Spacing between hint cells in `static` mode (minimum 20px).",
  },
  grid_color: {
    group: "Grid Mode",
    type: "hex color",
    desc: "Stroke and label color for the recursive 2x2 grid crosshair.",
  },
  grid_border_size: {
    group: "Grid Mode",
    type: "integer (px)",
    desc: "Crosshair line thickness in pixels.",
  },
  grid_font_size: {
    group: "Grid Mode",
    type: "float (px)",
    desc: "Font size for the quadrant labels inside the active grid cell.",
  },
  grid_min_size: {
    group: "Grid Mode",
    type: "float (px)",
    desc: "When cell width and height both fall below this size, warpd-rs auto-warps to the cell centre.",
  },
  grid_quadrant_keys: {
    group: "Grid Mode",
    type: "string[4]",
    desc: "Four keys for subdividing into Top-Left, Top-Right, Bottom-Left, and Bottom-Right quadrants.",
  },
  speed: {
    group: "Normal Mode",
    type: "integer (px/s)",
    desc: "Base pointer movement velocity in pixels per second.",
  },
  acceleration: {
    group: "Normal Mode",
    type: "integer",
    desc: "Acceleration parameter (warpd-compatible key).",
  },
  normal_move_keys: {
    group: "Normal Mode",
    type: "string[4]",
    desc: "Directional keys in `[left, down, up, right]` order.",
  },
  speed_modifier_key: {
    group: "Normal Mode",
    type: "string",
    desc: "Key held to boost normal-mode pointer speed (`SHIFT` matches both Left and Right Shift).",
  },
  speed_modifier_multiplier: {
    group: "Normal Mode",
    type: "float",
    desc: "Velocity multiplier while holding `speed_modifier_key`.",
  },
  cursor_color: {
    group: "Normal Mode",
    type: "hex color",
    desc: "Color of the cursor indicator dot and subtle crosshair lines.",
  },
  cursor_size: {
    group: "Normal Mode",
    type: "integer (px)",
    desc: "Radius of the circular cursor indicator in pixels.",
  },
  crosshair_line_width: {
    group: "Normal Mode",
    type: "integer (px)",
    desc: "Stroke width of the normal-mode crosshair guide lines.",
  },
  buttons: {
    group: "Mouse & Actions",
    type: "string[3]",
    desc: "Keys mapped to `[left, middle, right]` mouse button clicks.",
  },
  click_after_warp: {
    group: "Mouse & Actions",
    type: "boolean",
    desc: "When `true`, completing a hint selection or grid auto-warp automatically emits a left click.",
  },
};

function buildConfigPageHtml(configToml, configParsed) {
  const groups = ["Hint Mode", "Grid Mode", "Normal Mode", "Mouse & Actions"];
  const toc = [
    { id: "config-location", text: "Config Location & CLI", depth: 2 },
    ...groups.map((g) => ({ id: slugify(g), text: g, depth: 2 })),
    { id: "example-config", text: "Full config.example.toml", depth: 2 },
    { id: "compositor-binds", text: "Compositor Keybinds", depth: 2 },
  ];

  const tablesHtml = groups
    .map((group) => {
      const rows = Object.entries(configParsed)
        .filter(([k]) => (CONFIG_DESCRIPTIONS[k]?.group || "Other") === group)
        .map(([key, val]) => {
          const meta = CONFIG_DESCRIPTIONS[key] || { type: typeof val, desc: "" };
          const formattedVal = Array.isArray(val) ? `[${val.map((v) => `"${v}"`).join(", ")}]` : JSON.stringify(val);
          const isHex = typeof val === "string" && /^#[0-9a-f]{6,8}$/i.test(val);
          const swatch = isHex
            ? `<span class="color-swatch" style="background:${escapeHtml(val)}" aria-hidden="true"></span>`
            : "";
          const descHtml = meta.desc.replace(/`([^`]+)`/g, "<code>$1</code>");
          return `<tr>
            <td><code>${escapeHtml(key)}</code></td>
            <td><span class="type-pill">${escapeHtml(meta.type)}</span></td>
            <td>${swatch}<code>${escapeHtml(formattedVal)}</code></td>
            <td>${descHtml}</td>
          </tr>`;
        })
        .join("\n");

      return `<h2 id="${slugify(group)}"><a class="heading-anchor" href="#${slugify(group)}">#</a>${escapeHtml(group)}</h2>
      <div class="table-wrap"><table>
        <thead><tr><th>Key</th><th>Type</th><th>Default</th><th>Description</th></tr></thead>
        <tbody>${rows}</tbody>
      </table></div>`;
    })
    .join("\n");

  const highlightedToml = hljs.highlight(configToml, { language: "ini" }).value;
  const hyprSnippet = `bind = SUPER ALT, x, exec, warpd-rs --hint\nbind = SUPER ALT, g, exec, warpd-rs --grid\nbind = SUPER ALT, c, exec, warpd-rs --normal`;
  const swaySnippet = `bindsym $mod+Alt+x exec warpd-rs --hint\nbindsym $mod+Alt+g exec warpd-rs --grid\nbindsym $mod+Alt+c exec warpd-rs --normal`;

  const html = `
    <h1>Configuration Reference</h1>
    <p class="lead">Generated directly from <code>config.example.toml</code>. Every key is optional — omitted fields fall back to the compiled defaults.</p>

    <h2 id="config-location"><a class="heading-anchor" href="#config-location">#</a>Config Location &amp; CLI</h2>
    <p><code>warpd-rs</code> resolves configuration in the following order:</p>
    <ol>
      <li>Explicit path passed via <code>--config &lt;PATH&gt;</code> (skips default search locations)</li>
      <li><code>$XDG_CONFIG_HOME/warpd-rs/config.toml</code></li>
      <li><code>~/.config/warpd-rs/config.toml</code></li>
    </ol>
    <p>You can scaffold a fresh default config file anytime with:</p>
    <div class="code-block"><div class="code-block-bar"><span class="code-lang">bash</span><button class="copy-btn" type="button" data-copy>Copy</button></div><pre><code class="hljs language-bash">warpd-rs --generate-config
warpd-rs --generate-config --config ~/.config/warpd-rs/custom.toml</code></pre></div>

    ${tablesHtml}

    <h2 id="example-config"><a class="heading-anchor" href="#example-config">#</a>Full config.example.toml</h2>
    <div class="code-block"><div class="code-block-bar"><span class="code-lang">toml</span><button class="copy-btn" type="button" data-copy>Copy</button></div><pre><code class="hljs language-toml">${highlightedToml}</code></pre></div>

    <h2 id="compositor-binds"><a class="heading-anchor" href="#compositor-binds">#</a>Compositor Keybinds</h2>
    <h3>Hyprland</h3>
    <div class="code-block"><div class="code-block-bar"><span class="code-lang">ini</span><button class="copy-btn" type="button" data-copy>Copy</button></div><pre><code class="hljs language-ini">${hljs.highlight(hyprSnippet, { language: "ini" }).value}</code></pre></div>
    <h3>Sway</h3>
    <div class="code-block"><div class="code-block-bar"><span class="code-lang">ini</span><button class="copy-btn" type="button" data-copy>Copy</button></div><pre><code class="hljs language-ini">${hljs.highlight(swaySnippet, { language: "ini" }).value}</code></pre></div>
  `;

  return { html, toc };
}

function buildRecordingsPageHtml(availableMp4s) {
  const statsPath = path.join(REPO_ROOT, "docs", "stats.txt");
  const statsText = fs.existsSync(statsPath) ? fs.readFileSync(statsPath, "utf8").trim() : "";

  const recordings = [
    {
      id: "hint-static",
      file: "tile_static",
      title: "Hint Mode — Static Grid",
      cmd: "warpd-rs --hint",
      summary: "Fixed-interval x,y placement across the focused monitor (`hint_source = \"static\"`).",
    },
    {
      id: "hint-detect",
      file: "tile_detect",
      title: "Hint Mode — OpenCV Target Detection",
      cmd: "warpd-rs --hint # hint_source = \"detect\"",
      summary: "Captures the focused output with `zwlr_screencopy_manager_v1` and runs Canny edge + contour filtering to snap hints directly onto UI elements.",
    },
    {
      id: "grid-mode",
      file: "grid",
      title: "Grid Mode — Recursive Quadrant Subdivision",
      cmd: "warpd-rs --grid",
      summary: "Recursively halves the active screen region using `u / i / j / k` until precision is reached or `m` is pressed.",
    },
    {
      id: "normal-mode",
      file: "normal",
      title: "Normal Mode — Continuous Crosshair Navigation",
      cmd: "warpd-rs --normal",
      summary: "120fps `h / j / k / l` cursor steering with `Shift` speed boost, `m / , / .` mouse clicks, and `x` / `g` mode switching.",
    },
  ].filter((r) => availableMp4s.has(r.file));

  const toc = [
    ...recordings.map((r) => ({ id: r.id, text: r.title, depth: 2 })),
    { id: "detection-overlay", text: "OpenCV Debug Overlay & Stats", depth: 2 },
  ];

  const videosHtml = recordings
    .map(
      (r) => `
      <h2 id="${r.id}"><a class="heading-anchor" href="#${r.id}">#</a>${escapeHtml(r.title)}</h2>
      <p>${r.summary.replace(/`([^`]+)`/g, "<code>$1</code>")}</p>
      <div class="code-block"><div class="code-block-bar"><span class="code-lang">bash</span><button class="copy-btn" type="button" data-copy>Copy</button></div><pre><code class="hljs language-bash">${escapeHtml(r.cmd)}</code></pre></div>
      <figure class="doc-media">
        <video autoplay loop muted playsinline controls preload="metadata" src="/docs/${escapeHtml(r.file)}.mp4"></video>
        <figcaption>${escapeHtml(r.title)} — captured on Wayland</figcaption>
      </figure>
    `
    )
    .join("\n");

  const overlayExists = fs.existsSync(path.join(REPO_ROOT, "docs", "05_rects_overlay.png"));
  const overlayHtml = overlayExists
    ? `<figure class="doc-media"><img src="/docs/05_rects_overlay.png" alt="OpenCV detection pipeline final candidate overlay: pruned in red, forwarded targets in green" loading="lazy" /><figcaption>Final detection stage (05_rects_overlay.png): pruned candidates in red, forwarded hint targets in green.</figcaption></figure>`
    : "";

  const html = `
    <h1>Recordings &amp; OpenCV Diagnostics</h1>
    <p class="lead">Real Wayland screen captures and OpenCV detection debug artifacts sourced directly from the repository <code>docs/</code> directory. Want to try these interactively? <a href="/#playground">Launch the live browser playground &rarr;</a></p>
    ${videosHtml}
    <h2 id="detection-overlay"><a class="heading-anchor" href="#detection-overlay">#</a>OpenCV Debug Overlay &amp; Stats</h2>
    <p>Inspect intermediate OpenCV pipeline stages by setting <code>WARPDRS_DETECT_DEBUG=1</code> and <code>WARPDRS_DETECT_DEBUG_DIR=/tmp/warpd-dbg</code>.</p>
    ${overlayHtml}
    ${
      statsText
        ? `<div class="code-block"><div class="code-block-bar"><span class="code-lang">docs/stats.txt</span><button class="copy-btn" type="button" data-copy>Copy</button></div><pre><code class="hljs language-plaintext">${escapeHtml(statsText)}</code></pre></div>`
        : ""
    }
  `;

  return { html, toc };
}

function renderShell({ title, description, activeHref, navItems, contentHtml, toc, cargo }) {
  const sidebarNav = navItems
    .map((item) => {
      const active = item.href === activeHref ? ' class="active" aria-current="page"' : "";
      return `<a href="${escapeHtml(item.href)}"${active}><span class="nav-dot"></span>${escapeHtml(item.title)}</a>`;
    })
    .join("\n");

  const tocHtml =
    toc.length > 0
      ? `<aside class="docs-toc">
          <div class="toc-title">On this page</div>
          <nav>
            ${toc
              .map(
                (t) =>
                  `<a href="#${escapeHtml(t.id)}" class="toc-link depth-${t.depth}">${escapeHtml(t.text)}</a>`
              )
              .join("\n")}
          </nav>
        </aside>`
      : "";

  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>${escapeHtml(title)} — warpd-rs Docs</title>
    <meta name="description" content="${escapeHtml(description)}" />
    <meta name="theme-color" content="#11111b" />
    <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
    <link rel="preconnect" href="https://fonts.googleapis.com" />
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet" />
    <link rel="stylesheet" href="/docs/docs.css" />
  </head>
  <body>
    <header class="docs-header">
      <div class="docs-header-inner">
        <div class="header-left">
          <button class="mobile-menu-btn" type="button" id="menu-toggle" aria-label="Toggle navigation">&#9776;</button>
          <a class="brand" href="/">
            <img src="/favicon.svg" width="24" height="24" alt="" />
            <span class="brand-name">warpd-rs</span>
            <span class="version-pill">v${escapeHtml(cargo.version)}</span>
          </a>
          <span class="header-sep">/</span>
          <a class="docs-crumb" href="/docs/">Docs</a>
        </div>
        <div class="header-right">
          <button class="search-trigger" type="button" id="search-open">
            <span>Search docs...</span>
            <kbd>/</kbd>
          </button>
          <a class="header-link" href="/#playground">Interactive Demo</a>
          <a class="header-link github-link" href="${escapeHtml(cargo.repository)}" target="_blank" rel="noopener noreferrer">GitHub &#8599;</a>
        </div>
      </div>
    </header>

    <div class="docs-layout">
      <aside class="docs-sidebar" id="docs-sidebar">
        <div class="sidebar-group-label">Documentation</div>
        <nav class="sidebar-nav">
          ${sidebarNav}
        </nav>
        <div class="sidebar-group-label">Links</div>
        <nav class="sidebar-nav">
          <a href="/">Interactive Landing SPA</a>
          <a href="/#playground">Live Browser Sandbox</a>
          <a href="/#config-lab">Config Customizer</a>
          <a href="${escapeHtml(cargo.repository)}" target="_blank" rel="noopener noreferrer">GitHub Repository &#8599;</a>
          <a href="${escapeHtml(cargo.repository)}/releases" target="_blank" rel="noopener noreferrer">Releases (v${escapeHtml(cargo.version)}) &#8599;</a>
        </nav>
      </aside>

      <main class="docs-main">
        <article class="docs-prose">
          ${contentHtml}
        </article>
        <footer class="docs-footer">
          <span>warpd-rs v${escapeHtml(cargo.version)} · MIT License · Generated from repository README.md &amp; docs/</span>
          <div class="docs-footer-links">
            <a href="/">Home</a>
            <a href="${escapeHtml(cargo.repository)}" target="_blank" rel="noopener noreferrer">GitHub</a>
          </div>
        </footer>
      </main>

      ${tocHtml}
    </div>

    <div class="search-modal" id="search-modal" hidden>
      <div class="search-backdrop" id="search-backdrop"></div>
      <div class="search-dialog" role="dialog" aria-modal="true" aria-label="Search documentation">
        <div class="search-input-row">
          <input type="search" id="search-input" placeholder="Search warpd-rs docs, modes, config keys..." autocomplete="off" />
          <kbd>ESC</kbd>
        </div>
        <div class="search-results" id="search-results">
          <div class="search-empty">Type to search README, configuration keys, and diagnostics.</div>
        </div>
      </div>
    </div>

    <script src="/docs/docs.js" defer></script>
  </body>
</html>
`;
}

const DOCS_CSS = `:root {
  --bg: #11111b;
  --surface: #181825;
  --surface-2: #1e1e2e;
  --border: #313244;
  --border-soft: rgba(137, 180, 250, 0.18);
  --text: #cdd6f4;
  --text-muted: #a6adc8;
  --text-dim: #7f849c;
  --blue: #89b4fa;
  --mauve: #cba6f7;
  --pink: #f38ba8;
  --green: #a6e3a1;
  --peach: #fab387;
  --yellow: #f9e2af;
  --teal: #94e2d5;
  --font-sans: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  --font-mono: "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace;
}

* { box-sizing: border-box; }
html { scroll-behavior: smooth; color-scheme: dark; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: var(--font-sans);
  line-height: 1.68;
  -webkit-font-smoothing: antialiased;
}

a { color: var(--blue); text-decoration: none; }
a:hover { text-decoration: underline; }

.docs-header {
  position: sticky;
  top: 0;
  z-index: 40;
  height: 60px;
  background: rgba(17, 17, 27, 0.88);
  backdrop-filter: blur(14px);
  border-bottom: 1px solid var(--border);
}

.docs-header-inner {
  max-width: 1400px;
  height: 100%;
  margin: 0 auto;
  padding: 0 20px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.header-left, .header-right {
  display: flex;
  align-items: center;
  gap: 12px;
}

.mobile-menu-btn {
  display: none;
  background: var(--surface-2);
  border: 1px solid var(--border);
  color: var(--text);
  border-radius: 8px;
  padding: 6px 10px;
  cursor: pointer;
}

.brand {
  display: inline-flex;
  align-items: center;
  gap: 9px;
  color: var(--text);
  font-weight: 700;
  letter-spacing: -0.02em;
}
.brand:hover { text-decoration: none; }
.brand-name { font-family: var(--font-mono); font-size: 0.98rem; }

.version-pill {
  font-family: var(--font-mono);
  font-size: 0.72rem;
  font-weight: 600;
  padding: 2px 7px;
  border-radius: 999px;
  background: rgba(137, 180, 250, 0.14);
  color: var(--blue);
  border: 1px solid rgba(137, 180, 250, 0.3);
}

.header-sep { color: var(--border); font-weight: 600; }
.docs-crumb { color: var(--text-muted); font-size: 0.9rem; font-weight: 500; }

.search-trigger {
  display: inline-flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
  background: var(--surface);
  border: 1px solid var(--border);
  color: var(--text-dim);
  padding: 6px 10px 6px 12px;
  border-radius: 8px;
  font-size: 0.84rem;
  cursor: pointer;
  transition: border-color 0.15s ease, color 0.15s ease;
}
.search-trigger:hover { border-color: var(--blue); color: var(--text); }
.search-trigger kbd, .search-input-row kbd {
  font-family: var(--font-mono);
  font-size: 0.72rem;
  padding: 1px 6px;
  border-radius: 5px;
  background: var(--surface-2);
  border: 1px solid var(--border);
  color: var(--text-muted);
}

.header-link {
  color: var(--text-muted);
  font-size: 0.88rem;
  font-weight: 500;
  padding: 6px 10px;
  border-radius: 8px;
  transition: background 0.15s, color 0.15s;
}
.header-link:hover { background: var(--surface-2); color: var(--text); text-decoration: none; }
.github-link { border: 1px solid var(--border); }

.docs-layout {
  max-width: 1400px;
  margin: 0 auto;
  display: grid;
  grid-template-columns: 260px minmax(0, 1fr) 230px;
  gap: 32px;
  padding: 28px 20px 64px;
}

.docs-sidebar {
  position: sticky;
  top: 84px;
  align-self: start;
  max-height: calc(100vh - 104px);
  overflow-y: auto;
  padding-right: 12px;
}

.sidebar-group-label {
  font-family: var(--font-mono);
  font-size: 0.72rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--text-dim);
  margin: 20px 0 8px 10px;
}
.sidebar-group-label:first-child { margin-top: 0; }

.sidebar-nav { display: flex; flex-direction: column; gap: 3px; }
.sidebar-nav a {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 8px;
  color: var(--text-muted);
  font-size: 0.9rem;
  font-weight: 500;
  transition: background 0.15s, color 0.15s;
}
.sidebar-nav a:hover { background: var(--surface); color: var(--text); text-decoration: none; }
.sidebar-nav a.active {
  background: rgba(137, 180, 250, 0.12);
  color: var(--blue);
  border: 1px solid rgba(137, 180, 250, 0.25);
}
.nav-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
  opacity: 0.65;
  flex-shrink: 0;
}

.docs-main { min-width: 0; }

.docs-prose {
  max-width: 820px;
}
.docs-prose h1 {
  font-size: clamp(1.9rem, 3vw, 2.5rem);
  line-height: 1.18;
  letter-spacing: -0.03em;
  margin: 0 0 14px;
  color: #fff;
}
.docs-prose .lead {
  font-size: 1.05rem;
  color: var(--text-muted);
  margin-bottom: 28px;
}
.docs-prose h2 {
  font-size: 1.45rem;
  letter-spacing: -0.02em;
  margin: 44px 0 14px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border);
  scroll-margin-top: 84px;
  color: #eef2ff;
}
.docs-prose h3 {
  font-size: 1.15rem;
  margin: 30px 0 10px;
  scroll-margin-top: 84px;
  color: var(--blue);
}
.docs-prose h4 {
  font-size: 1rem;
  margin: 22px 0 8px;
  scroll-margin-top: 84px;
  color: var(--mauve);
}
.heading-anchor {
  color: var(--text-dim);
  margin-right: 8px;
  font-family: var(--font-mono);
  font-weight: 400;
  opacity: 0.35;
}
h2:hover .heading-anchor, h3:hover .heading-anchor, h4:hover .heading-anchor {
  opacity: 1;
  color: var(--blue);
  text-decoration: none;
}

.docs-prose p { margin: 0 0 16px; color: var(--text); }
.docs-prose ul, .docs-prose ol { margin: 0 0 18px; padding-left: 22px; }
.docs-prose li { margin-bottom: 6px; }
.docs-prose strong { color: #fff; }

.docs-prose code:not(.hljs) {
  font-family: var(--font-mono);
  font-size: 0.86em;
  background: var(--surface-2);
  border: 1px solid var(--border);
  color: var(--mauve);
  padding: 2px 6px;
  border-radius: 6px;
}

.code-block {
  margin: 18px 0 24px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  overflow: hidden;
}
.code-block-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 14px;
  background: var(--surface-2);
  border-bottom: 1px solid var(--border);
}
.code-lang {
  font-family: var(--font-mono);
  font-size: 0.74rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-dim);
}
.copy-btn {
  font-family: var(--font-mono);
  font-size: 0.74rem;
  background: transparent;
  border: 1px solid var(--border);
  color: var(--text-muted);
  border-radius: 6px;
  padding: 3px 9px;
  cursor: pointer;
}
.copy-btn:hover { border-color: var(--blue); color: var(--text); }
.code-block pre {
  margin: 0;
  padding: 16px;
  overflow-x: auto;
  font-family: var(--font-mono);
  font-size: 0.84rem;
  line-height: 1.6;
}

.doc-media {
  margin: 18px 0 28px;
  padding: 12px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 14px;
}
.doc-media video, .doc-media img {
  display: block;
  width: 100%;
  max-height: 480px;
  object-fit: contain;
  border-radius: 8px;
  background: #0b0b12;
}
.doc-media figcaption {
  margin-top: 10px;
  font-family: var(--font-mono);
  font-size: 0.78rem;
  color: var(--text-dim);
  text-align: center;
}

.table-wrap {
  overflow-x: auto;
  margin: 14px 0 28px;
  border: 1px solid var(--border);
  border-radius: 12px;
  background: var(--surface);
}
table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.88rem;
}
th, td {
  text-align: left;
  padding: 11px 14px;
  border-bottom: 1px solid var(--border);
  vertical-align: top;
}
th {
  font-family: var(--font-mono);
  font-size: 0.76rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-dim);
  background: var(--surface-2);
}
tr:last-child td { border-bottom: none; }

.type-pill {
  font-family: var(--font-mono);
  font-size: 0.74rem;
  color: var(--teal);
  background: rgba(148, 226, 213, 0.1);
  border: 1px solid rgba(148, 226, 213, 0.25);
  border-radius: 6px;
  padding: 2px 6px;
  white-space: nowrap;
}

.color-swatch {
  display: inline-block;
  width: 12px;
  height: 12px;
  border-radius: 3px;
  border: 1px solid rgba(255, 255, 255, 0.25);
  margin-right: 6px;
  vertical-align: middle;
}

.docs-toc {
  position: sticky;
  top: 84px;
  align-self: start;
  max-height: calc(100vh - 104px);
  overflow-y: auto;
  border-left: 1px solid var(--border);
  padding-left: 16px;
}
.toc-title {
  font-family: var(--font-mono);
  font-size: 0.72rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--text-dim);
  margin-bottom: 10px;
}
.docs-toc nav { display: flex; flex-direction: column; gap: 6px; }
.toc-link {
  font-size: 0.82rem;
  color: var(--text-dim);
  line-height: 1.35;
}
.toc-link.depth-3 { padding-left: 12px; font-size: 0.78rem; }
.toc-link:hover, .toc-link.active { color: var(--blue); text-decoration: none; }

.docs-footer {
  margin-top: 56px;
  padding-top: 20px;
  border-top: 1px solid var(--border);
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 12px;
  font-size: 0.82rem;
  color: var(--text-dim);
}
.docs-footer-links { display: flex; gap: 16px; }

/* Search modal */
.search-modal[hidden] { display: none; }
.search-modal {
  position: fixed;
  inset: 0;
  z-index: 80;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: 10vh 16px 24px;
}
.search-backdrop {
  position: absolute;
  inset: 0;
  background: rgba(11, 11, 18, 0.75);
  backdrop-filter: blur(6px);
}
.search-dialog {
  position: relative;
  width: 100%;
  max-width: 600px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 14px;
  overflow: hidden;
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.55);
}
.search-input-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border);
}
.search-input-row input {
  flex: 1;
  background: transparent;
  border: none;
  outline: none;
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 0.98rem;
}
.search-results {
  max-height: 400px;
  overflow-y: auto;
  padding: 8px;
}
.search-empty {
  padding: 20px;
  text-align: center;
  color: var(--text-dim);
  font-size: 0.88rem;
}
.search-hit {
  display: block;
  padding: 10px 12px;
  border-radius: 8px;
  color: var(--text);
}
.search-hit:hover { background: var(--surface-2); text-decoration: none; }
.search-hit-title { font-weight: 600; color: var(--blue); font-size: 0.92rem; }
.search-hit-meta { font-family: var(--font-mono); font-size: 0.74rem; color: var(--text-dim); margin-top: 2px; }
.search-hit-snippet { font-size: 0.82rem; color: var(--text-muted); margin-top: 4px; }

/* Catppuccin Mocha highlight.js tokens */
.hljs { color: #cdd6f4; background: transparent; }
.hljs-comment, .hljs-quote { color: #6c7086; font-style: italic; }
.hljs-keyword, .hljs-selector-tag, .hljs-section { color: #cba6f7; }
.hljs-string, .hljs-meta-string { color: #a6e3a1; }
.hljs-number, .hljs-literal, .hljs-variable { color: #fab387; }
.hljs-attr, .hljs-attribute, .hljs-name { color: #89b4fa; }
.hljs-built_in, .hljs-title { color: #f9e2af; }
.hljs-symbol, .hljs-bullet { color: #f38ba8; }

@media (max-width: 1120px) {
  .docs-layout { grid-template-columns: 230px minmax(0, 1fr); }
  .docs-toc { display: none; }
}
@media (max-width: 820px) {
  .mobile-menu-btn { display: inline-flex; }
  .docs-layout { grid-template-columns: minmax(0, 1fr); }
  .docs-sidebar {
    display: none;
    position: fixed;
    top: 60px;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 35;
    max-height: none;
    background: var(--bg);
    padding: 20px;
  }
  .docs-sidebar.open { display: block; }
  .header-link:not(.github-link) { display: none; }
  .search-trigger span { display: none; }
}
`;

const DOCS_JS = `(() => {
  const menuBtn = document.getElementById("menu-toggle");
  const sidebar = document.getElementById("docs-sidebar");
  if (menuBtn && sidebar) {
    menuBtn.addEventListener("click", () => sidebar.classList.toggle("open"));
  }

  document.querySelectorAll("[data-copy]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const code = btn.closest(".code-block")?.querySelector("pre code")?.textContent || "";
      try {
        await navigator.clipboard.writeText(code);
        const orig = btn.textContent;
        btn.textContent = "Copied!";
        setTimeout(() => { btn.textContent = orig; }, 1400);
      } catch {
        btn.textContent = "Failed";
      }
    });
  });

  // TOC active state
  const tocLinks = Array.from(document.querySelectorAll(".toc-link"));
  if (tocLinks.length > 0 && "IntersectionObserver" in window) {
    const headings = tocLinks
      .map((a) => document.getElementById(a.getAttribute("href").slice(1)))
      .filter(Boolean);
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            tocLinks.forEach((a) => a.classList.toggle("active", a.getAttribute("href") === "#" + entry.target.id));
          }
        }
      },
      { rootMargin: "-80px 0px -70% 0px" }
    );
    headings.forEach((h) => observer.observe(h));
  }

  // Search modal
  const modal = document.getElementById("search-modal");
  const openBtn = document.getElementById("search-open");
  const backdrop = document.getElementById("search-backdrop");
  const input = document.getElementById("search-input");
  const resultsEl = document.getElementById("search-results");
  let indexPromise = null;

  const loadIndex = () => {
    if (!indexPromise) {
      indexPromise = fetch("/docs/search-index.json").then((r) => r.json()).catch(() => []);
    }
    return indexPromise;
  };

  const openModal = () => {
    if (!modal) return;
    modal.hidden = false;
    loadIndex();
    setTimeout(() => input?.focus(), 10);
  };
  const closeModal = () => {
    if (!modal) return;
    modal.hidden = true;
  };

  openBtn?.addEventListener("click", openModal);
  backdrop?.addEventListener("click", closeModal);

  window.addEventListener("keydown", (e) => {
    if ((e.key === "/" && document.activeElement?.tagName !== "INPUT" && document.activeElement?.tagName !== "TEXTAREA") || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k")) {
      e.preventDefault();
      openModal();
    } else if (e.key === "Escape" && modal && !modal.hidden) {
      closeModal();
    }
  });

  input?.addEventListener("input", async () => {
    const q = input.value.trim().toLowerCase();
    const docs = await loadIndex();
    if (!q) {
      resultsEl.innerHTML = '<div class="search-empty">Type to search README, configuration keys, and diagnostics.</div>';
      return;
    }
    const hits = docs.filter((d) =>
      d.title.toLowerCase().includes(q) ||
      d.pageTitle.toLowerCase().includes(q) ||
      d.text.toLowerCase().includes(q)
    ).slice(0, 12);

    if (hits.length === 0) {
      resultsEl.innerHTML = '<div class="search-empty">No matching docs entries found.</div>';
      return;
    }

    resultsEl.innerHTML = hits
      .map((h) => {
        const idx = h.text.toLowerCase().indexOf(q);
        const start = Math.max(0, idx - 40);
        const snippet = h.text.slice(start, start + 140);
        const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
        return '<a class="search-hit" href="' + esc(h.href) + '">' +
          '<div class="search-hit-title">' + esc(h.title) + '</div>' +
          '<div class="search-hit-meta">' + esc(h.pageTitle) + '</div>' +
          '<div class="search-hit-snippet">' + esc(snippet) + (snippet.length >= 140 ? "..." : "") + '</div>' +
        '</a>';
      })
      .join("");
  });
})();
`;

function main() {
  const cargo = parseCargoMeta();
  const { availableMp4s, files } = copyDocsAssets();
  const mdRenderer = createMarkdownRenderer(availableMp4s);

  const readmeMd = fs.readFileSync(path.join(REPO_ROOT, "README.md"), "utf8");
  const configToml = fs.readFileSync(path.join(REPO_ROOT, "config.example.toml"), "utf8").trim();
  const configParsed = parseConfigToml(configToml);

  const extraMdFiles = files
    .filter((f) => f.toLowerCase().endsWith(".md"))
    .sort((a, b) => a.localeCompare(b));

  const extraPages = extraMdFiles.map((file) => {
    const raw = fs.readFileSync(path.join(REPO_ROOT, "docs", file), "utf8");
    const slug = file.replace(/\.md$/i, "").toLowerCase().replace(/[^a-z0-9]+/g, "-");
    const h1 = raw.match(/^#\s+(.+)$/m);
    const title = h1
      ? h1[1].trim()
      : slug
          .split("-")
          .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
          .join(" ");
    const rendered = mdRenderer.render(raw);
    return {
      slug,
      title,
      href: `/docs/${slug}/`,
      description: `${title} — warpd-rs documentation`,
      html: rendered.html,
      toc: rendered.toc,
    };
  });

  const navItems = [
    { title: "Overview (README)", href: "/docs/" },
    { title: "Configuration", href: "/docs/configuration/" },
    { title: "Recordings & OpenCV", href: "/docs/recordings/" },
    ...extraPages.map((p) => ({ title: p.title, href: p.href })),
  ];

  const readmeRendered = mdRenderer.render(readmeMd);
  const configRendered = buildConfigPageHtml(configToml, configParsed);
  const recordingsRendered = buildRecordingsPageHtml(availableMp4s);

  const pages = [
    {
      outPath: path.join(DOCS_OUT, "index.html"),
      title: "Overview",
      description: `Documentation for ${cargo.name}: ${cargo.description}`,
      href: "/docs/",
      html: readmeRendered.html,
      toc: readmeRendered.toc,
    },
    {
      outPath: path.join(DOCS_OUT, "configuration", "index.html"),
      title: "Configuration Reference",
      description: "Complete TOML configuration keys, defaults, and compositor bindings for warpd-rs.",
      href: "/docs/configuration/",
      html: configRendered.html,
      toc: configRendered.toc,
    },
    {
      outPath: path.join(DOCS_OUT, "recordings", "index.html"),
      title: "Recordings & OpenCV Diagnostics",
      description: "Real Wayland captures and OpenCV detection pipeline debug visuals for warpd-rs.",
      href: "/docs/recordings/",
      html: recordingsRendered.html,
      toc: recordingsRendered.toc,
    },
    ...extraPages.map((p) => ({
      outPath: path.join(DOCS_OUT, p.slug, "index.html"),
      title: p.title,
      description: p.description,
      href: p.href,
      html: p.html,
      toc: p.toc,
    })),
  ];

  const searchIndex = [];

  for (const page of pages) {
    fs.mkdirSync(path.dirname(page.outPath), { recursive: true });
    const fullHtml = renderShell({
      title: page.title,
      description: page.description,
      activeHref: page.href,
      navItems,
      contentHtml: page.html,
      toc: page.toc,
      cargo,
    });
    fs.writeFileSync(page.outPath, fullHtml, "utf8");

    const plainText = stripHtml(page.html);
    searchIndex.push({
      title: page.title,
      pageTitle: page.title,
      href: page.href,
      text: plainText.slice(0, 600),
    });

    for (const item of page.toc) {
      searchIndex.push({
        title: item.text,
        pageTitle: page.title,
        href: `${page.href}#${item.id}`,
        text: `${item.text} ${plainText.slice(0, 350)}`,
      });
    }
  }

  // Also index every config key directly for crisp config lookup
  for (const [key, val] of Object.entries(configParsed)) {
    const meta = CONFIG_DESCRIPTIONS[key];
    searchIndex.push({
      title: key,
      pageTitle: "Configuration Reference",
      href: `/docs/configuration/#${slugify(meta?.group || "hint-mode")}`,
      text: `${key} = ${JSON.stringify(val)} — ${meta?.desc || ""}`,
    });
  }

  fs.writeFileSync(path.join(DOCS_OUT, "docs.css"), DOCS_CSS, "utf8");
  fs.writeFileSync(path.join(DOCS_OUT, "docs.js"), DOCS_JS, "utf8");
  fs.writeFileSync(path.join(DOCS_OUT, "search-index.json"), JSON.stringify(searchIndex), "utf8");

  console.log(
    `[build-docs] built ${pages.length} static doc pages + ${searchIndex.length} search entries into ${path.relative(REPO_ROOT, DOCS_OUT)}`
  );
}

main();
