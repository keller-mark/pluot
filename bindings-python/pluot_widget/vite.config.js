import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import anywidget from "@anywidget/vite";

export default defineConfig({
  build: {
    outDir: "src/pluot_widget/static",
    lib: {
      entry: ["js/widget.tsx"],
      formats: ["es"],
      fileName: "widget",
    },
    rollupOptions: {
      // anywidget loads `_esm` as a single module, so chunks such as the
      // lazily-imported fonts from @pluot/core cannot be resolved relative to it.
      output: { inlineDynamicImports: true },
    },
  },
  define: {
    "process.env.NODE_ENV": JSON.stringify(process.env.NODE_ENV ?? "production"),
    // For 3d-view-controls.
    global: "window",
  },
  plugins: [react(), anywidget()],
});
