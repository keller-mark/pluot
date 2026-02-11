import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import serveStatic from "serve-static";
import { resolve } from "path";

/**
 * Vite plugins to serves contents of `packages/file-types/zarr/fixtures` during testing.
 * Reference: https://github.com/hms-dbmi/viv/blob/d8b0ae/sites/avivator/vite.config.js#L12
 */
export function serveDemoFixtures() {
  const serveOptions = {
    setHeaders: (res) => {
      res.setHeader("Access-Control-Allow-Origin", "*");
    },
    dotfiles: "allow",
    acceptRanges: true,
    immutable: true,
    index: false,
    maxAge: 1000 * 60 * 60 * 24, // 24 hours
  };
  const dirZarr = resolve(__dirname, "../../data/out");
  console.log(`Serving demo data from: ${dirZarr}`);
  const serveZarr = serveStatic(dirZarr, serveOptions);
  return {
    name: "serve-demo-data-dir",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (/^\/@data\//.test(req.url)) {
          req.url = req.url.replace("/@data/", "");
          serveZarr(req, res, next);
        } else {
          next();
        }
      });
    },
  };
}

export default defineConfig({
  base: "/",
  build: {
    target: "esnext",
  },
  plugins: [
    wasm(),
    react({
      jsxRuntime: "classic",
    }),
    serveDemoFixtures(),
    {
      // Fix for error in Firefox:
      //   DOMException: Worker.postMessage: The WebAssembly.Memory object cannot be serialized.
      //   The Cross-Origin-Opener-Policy and Cross-Origin-Embedder-Policy HTTP headers can be used to enable this.
      // Reference: https://github.com/vitejs/vite/issues/3909#issuecomment-934044912
      name: "configure-response-headers",
      configureServer: (server) => {
        server.middlewares.use((_req, res, next) => {
          res.setHeader("Cross-Origin-Embedder-Policy", "require-corp");
          res.setHeader("Cross-Origin-Opener-Policy", "same-origin");
          next();
        });
      },
    },
  ],
  // To enable .js files that contain JSX to be imported.
  // Reference: https://github.com/vitest-dev/vitest/issues/1564
  esbuild: {
    loader: "tsx",
    include: /src\/.*\.[tj]sx?$/,
    exclude: [],
  },
});
