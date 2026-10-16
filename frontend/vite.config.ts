import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Relative base: the same build runs under rhine://app/ (backend) and
// http://localhost (dev / preview in a normal browser with the mock backend).
export default defineConfig({
  base: "./",
  plugins: [svelte()],
  build: {
    target: "safari16",
    assetsInlineLimit: 0,
    chunkSizeWarningLimit: 2000,
    sourcemap: false,
  },
  server: { port: 5173, strictPort: true },
});
