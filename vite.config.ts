import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";

// Tauri expects a fixed dev port and does not want vite to clear the screen.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      // Rust source is watched by tauri, not vite.
      ignored: ["**/src-tauri/**"],
    },
  },
  // Produce assets Tauri can bundle; env vars prefixed VITE_ or TAURI_ are exposed.
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "chrome110",
    minify: process.env.TAURI_DEBUG ? false : "esbuild",
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});
