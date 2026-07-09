import { defineConfig } from "vite";

// The dev server proxies /api to the Rust backend on :8080 so the frontend and
// backend can be developed independently. In production the same origin serves
// both, so the relative /api paths in src/api.ts work unchanged.
export default defineConfig({
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8080",
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: "dist",
    target: "es2022",
    // Monaco's editor core is ~2 MB on its own; that is expected, not a mistake.
    chunkSizeWarningLimit: 3000,
  },
  worker: {
    format: "es",
  },
});
