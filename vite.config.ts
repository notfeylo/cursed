import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwind from "@tailwindcss/vite";

// Tauri expects a fixed port and no obfuscated sourcemaps in dev.
export default defineConfig({
  plugins: [react(), tailwind()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "chrome110",
    // Vite 8 bundles with rolldown and no longer ships esbuild; "oxc" is the
    // minifier that replaces it.
    minify: "oxc",
    sourcemap: false,
    chunkSizeWarningLimit: 900,
  },
});
