import { defineConfig } from "cf/config";

export default defineConfig({
  worker: {
    name: "warpd-rs-site",
    compatibilityDate: "2026-11-22",
    domains: ["warpd.jesse.rs"],
    assets: {
      htmlHandling: "auto-trailing-slash",
      notFoundHandling: "single-page-application",
    },
  },
});
