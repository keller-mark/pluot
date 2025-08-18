import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import wasm from "vite-plugin-wasm";


export default defineConfig({
  base: '/pluot/',
  build: {
    target: "esnext",
  },
  plugins: [wasm(), react({
    jsxRuntime: 'classic',
  })],
  // To enable .js files that contain JSX to be imported.
  // Reference: https://github.com/vitest-dev/vitest/issues/1564
  esbuild: {
    loader: 'tsx',
    include: /src\/.*\.[tj]sx?$/,
    exclude: [],
  },
});