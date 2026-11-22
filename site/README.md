# warpd-rs Web Showcase & Static Docs (`warpd.jesse.rs`)

Deployed to **Cloudflare Workers + Workers Static Assets** with Custom Domain `warpd.jesse.rs` via `cloudflare.config.ts` and the `cf` CLI.

## Architecture

- **`/` (Landing SPA)**: React + TypeScript + Vite interactive showcase with:
  - Live Wayland desktop sandbox for **Hint (Static)**, **Hint (OpenCV Detect)** with 4-stage pipeline visualization, **Grid** recursive subdivision, and **Normal** 120fps crosshair modes (both guided Auto-Play and live keyboard input)
  - Real Wayland video captures & OpenCV debug metrics synced from `../docs/`
  - Live `config.toml` customizer synced directly into the playground
- **`/docs/` (Static Docs Site)**: Generated at build time by `scripts/build-docs.mjs` directly from:
  - `../README.md` -> `/docs/`
  - `../config.example.toml` -> `/docs/configuration/`
  - `../docs/*` (videos, images, `stats.txt`) -> `/docs/recordings/` & `/docs/<asset>`
  - Any future `../docs/*.md` files -> automatically discovered and rendered at `/docs/<slug>/`
  - Client-side instant search index at `/docs/search-index.json`

## Local Development

```bash
just site-install
just site-dev
```

## Build & Dry-Run Validation

```bash
just site-build
just site-deploy-dry-run
```

## Publishing

### Option A: Automated CI/CD (GitHub Actions)

`.github/workflows/deploy-site.yml` automatically builds, typechecks, dry-run validates PRs, and deploys on pushes to `main` whenever `site/**`, `README.md`, `docs/**`, `config.example.toml`, or `Cargo.toml` change.

Add two repository secrets in **GitHub → Settings → Secrets and variables → Actions**:
1. `CLOUDFLARE_ACCOUNT_ID` — your Cloudflare account ID
2. `CLOUDFLARE_API_TOKEN` — an Account API Token with **Workers Scripts: Edit** and **Zone → Workers Routes / Custom Domains** permissions scoped to `jesse.rs`

Cloudflare Custom Domains (`domains: ["warpd.jesse.rs"]` in `cloudflare.config.ts`) automatically provision the DNS record and TLS certificate on `jesse.rs`.

### Option B: Manual Deploy from Terminal

```bash
cd site
npx cf auth login
npx cf deploy ./dist
```
