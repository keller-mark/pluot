import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { resolve } from "path";
import { existsSync } from "fs";

const cwd = process.cwd();

const mainFiles = ["index.tsx", "index.ts", "index.jsx", "index.js"];

const indexFile = mainFiles.find((d) => existsSync(resolve(cwd, `src/${d}`)));

// For bundling "sub-packages".
export default defineConfig({
  root: cwd,
  build: {
    emptyOutDir: true,
    minify: false,
    sourcemap: false,
    lib: {
      entry: resolve(cwd, `src/${indexFile}`),
      // The file extension used by Vite depends on whether the package.json contains "type": "module".
      // Reference: https://github.com/vitejs/vite/blob/1ee0014caa7ecf91ac147dca3801820020a4b8a0/docs/guide/build.md?plain=1#L212
      fileName: "index",
      formats: ["es"],
    },
    rollupOptions: {
      external: ["react", "react-dom"],
      output: {
        globals: {
          react: "React",
          "react-dom": "ReactDOM",
        },
      },
    },
  },
  define: {
    "process.env.NODE_ENV": `"${process.env.APP_ENV}"`,
    // For 3d-view-controls.
    global: "window",
  },
  plugins: [react()],
  // To enable .js files that contain JSX to be imported.
  // Reference: https://github.com/vitest-dev/vitest/issues/1564
  esbuild: {
    loader: "tsx",
    include: /src\/.*\.[tj]sx?$/,
    exclude: [],
  },
});
