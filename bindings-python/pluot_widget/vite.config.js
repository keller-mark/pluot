import { readFileSync } from "fs";
import { defineConfig } from "vite";
import anywidget from "@anywidget/vite";

const readVersion = (pkg) => JSON.parse(readFileSync(new URL(`./node_modules/${pkg}/package.json`, import.meta.url))).version;

export default defineConfig({
  server: {
    port: 5183,
    // widget.py hardcodes this port for ANYWIDGET_HMR.
    strictPort: true,
  },
  build: {
    outDir: "src/pluot_widget/static",
    // For the top-level await in js/deps.ts.
    target: "es2022",
    lib: {
      entry: ["js/widget.tsx"],
      formats: ["es"],
      fileName: "widget",
    },
  },
  define: {
    __REACT_VERSION__: JSON.stringify(readVersion("react")),
    __PLUOT_REACT_VERSION__: JSON.stringify(readVersion("@pluot/react")),
  },
  plugins: [anywidget()],
});
