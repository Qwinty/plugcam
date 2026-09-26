import { defineConfig } from "vite";
import { svelte, vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import pkg from "./package.json" with { type: "json" };

// Tauri expects a fixed port and does not need the Vite overlay to clear the screen.
export default defineConfig({
  root: "app",
  plugins: [svelte({ preprocess: vitePreprocess() })],
  define: { __APP_VERSION__: JSON.stringify(pkg.version) },
  clearScreen: false,
  // IPv4 explicitly: Node resolves "localhost" to ::1, Tauri checks 127.0.0.1.
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
  build: { outDir: "../dist", emptyOutDir: true, target: "chrome120" },
});
