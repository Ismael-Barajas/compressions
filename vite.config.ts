/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const host = process.env.TAURI_DEV_HOST;
const { version: appVersion } = JSON.parse(
  readFileSync(resolve(root, "package.json"), "utf-8"),
) as { version: string };

export default defineConfig({
  plugins: [react()],
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
  },
  resolve: {
    alias: {
      "@": resolve(root, "src"),
    },
  },
  clearScreen: false,
  build: {
    // Tauri v2 webview floor: WebView2 (Chromium) on Windows, WebKit on macOS 10.15+/Linux.
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: "esbuild",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    include: ["tests/**/*.test.ts"],
    benchmark: {
      include: ["tests/**/*.bench.ts"],
    },
    coverage: {
      provider: "v8",
      include: ["src/**/*.ts", "src/**/*.tsx"],
    },
  },
});
