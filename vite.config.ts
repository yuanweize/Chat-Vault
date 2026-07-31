import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
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
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    // Markdown + KaTeX is intentionally lazy-loaded with ChatView. Its 600 kB
    // parsed chunk is ~180 kB gzip and never blocks the account picker.
    chunkSizeWarningLimit: 650,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          if (id.includes("react-syntax-highlighter") || id.includes("refractor") || id.includes("highlight.js") || id.includes("prismjs") || id.includes("lowlight")) return "syntax";
          if (id.includes("react-markdown") || id.includes("remark-") || id.includes("rehype-") || id.includes("katex") || id.includes("unified") || id.includes("micromark") || id.includes("mdast-") || id.includes("hast-") || id.includes("vfile") || id.includes("property-information")) return "markdown";
          if (id.includes("react-virtuoso")) return "virtuoso";
          if (id.includes("jszip")) return "export";
          if (id.includes("i18next")) return "i18n";
          if (id.includes("react") || id.includes("scheduler")) return "react";
          if (id.includes("@tauri-apps")) return "tauri";
          return undefined;
        },
      },
    },
  },
}));
