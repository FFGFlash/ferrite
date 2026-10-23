import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Tailored for Tauri: tauri.conf.json points `devUrl` at this fixed port, and
// the screen is not cleared so Rust build output stays visible.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/target/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    // WebView2 on Windows is evergreen Chromium; WKWebView and WebKitGTK lag behind.
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome111" : "safari15",
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
  },
});
