import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Frontend standalone — connects to backend at http://127.0.0.1:9420
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  cacheDir: process.env.YILIAN_E2E_VITE_CACHE_DIR,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: "dist",
  },
});
