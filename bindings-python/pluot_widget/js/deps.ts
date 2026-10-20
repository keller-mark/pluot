// Loaded from CDNs rather than bundled, which keeps the widget's _esm small.
// In marimo-wasm, a large inline _esm costs hundreds of MB of Pyodide heap (see dev-docs/README.md).
type ImportMap = { imports: Record<string, string> };
type ImportWithMap = (moduleName: string, importMap: ImportMap) => Promise<any>;

declare const __REACT_VERSION__: string;
declare const __PLUOT_REACT_VERSION__: string;

const importUrl = (url: string): Promise<any> => import(/* @vite-ignore */ url);

// Reference: https://github.com/vitessce/vitessce-python/blob/1fdcf8a238a5e22cdfad21ef2f679c03154f000c/src/vitessce/widget.py#L162
let importWithMap: ImportWithMap;
try {
  importWithMap = (await importUrl("https://unpkg.com/dynamic-importmap@0.1.0")).importWithMap;
} catch (e) {
  console.warn("Import of dynamic-importmap failed, trying fallback.");
  importWithMap = (await importUrl("https://cdn.vitessce.io/dynamic-importmap@0.1.0/dist/index.js")).importWithMap;
}

// Each CDN's react-dom imports its own copy of react, so react must be mapped
// to that same CDN's URL to end up with a single React instance.
function getImportMap(jsDevMode: boolean, pluotReactVersion: string): ImportMap {
  const dev = jsDevMode ? "?dev" : "";
  return {
    imports: {
      "react": `https://esm.sh/react@${__REACT_VERSION__}${dev}`,
      "react/jsx-runtime": `https://esm.sh/react@${__REACT_VERSION__}/jsx-runtime${dev}`,
      "react-dom/client": `https://esm.sh/react-dom@${__REACT_VERSION__}/client${dev}`,
      "@pluot/react": `https://cdn.jsdelivr.net/npm/@pluot/react@${pluotReactVersion}/dist/index.js`,
    },
  };
}

// jsDelivr only serves production builds of React as ES modules, so js_dev_mode
// falls back to production React.
function getFallbackImportMap(pluotReactVersion: string): ImportMap {
  return {
    imports: {
      "react": `https://cdn.jsdelivr.net/npm/react@${__REACT_VERSION__}/+esm`,
      "react/jsx-runtime": `https://cdn.jsdelivr.net/npm/react@${__REACT_VERSION__}/jsx-runtime/+esm`,
      "react-dom/client": `https://cdn.jsdelivr.net/npm/react-dom@${__REACT_VERSION__}/client/+esm`,
      "@pluot/react": `https://unpkg.com/@pluot/react@${pluotReactVersion}/dist/index.js`,
    },
  };
}

const successfulImportMap: ImportMap = { imports: {} };

// Dependencies resolve to their previously successful URLs, but the requested module itself
// always comes from importMap, since its version or custom_js_url can differ between widgets.
async function importAndRecord(moduleName: string, importMap: ImportMap) {
  const url = importMap.imports[moduleName];
  const result = await importWithMap(moduleName, {
    imports: { ...importMap.imports, ...successfulImportMap.imports, [moduleName]: url },
  });
  successfulImportMap.imports[moduleName] = url;
  return result;
}

async function importWithMapAndFallback(moduleName: string, importMap: ImportMap, fallbackMap: ImportMap | null) {
  if (!fallbackMap) {
    return importAndRecord(moduleName, importMap);
  }
  try {
    return await importAndRecord(moduleName, importMap);
  } catch (e) {
    console.warn(`Importing ${moduleName} failed with importMap`, importMap, "trying fallback", fallbackMap, successfulImportMap);
    return importAndRecord(moduleName, fallbackMap);
  }
}

export type PluotReactModule = typeof import("@pluot/react");

// Assigned by loadDeps, and only read inside components and hooks, which run after it resolves.
export let React: typeof import("react");
export let ReactDOMClient: typeof import("react-dom/client");

// A page can only use one React instance, so the first widget to render decides js_dev_mode.
let reactDevModePromise: Promise<boolean> | null = null;

async function loadReact(jsDevMode: boolean): Promise<boolean> {
  reactDevModePromise ??= (async () => {
    const importMap = getImportMap(jsDevMode, __PLUOT_REACT_VERSION__);
    const fallbackImportMap = getFallbackImportMap(__PLUOT_REACT_VERSION__);
    React = await importWithMapAndFallback("react", importMap, fallbackImportMap);
    ReactDOMClient = await importWithMapAndFallback("react-dom/client", importMap, fallbackImportMap);
    return jsDevMode;
  })();
  const loadedDevMode = await reactDevModePromise;
  if (loadedDevMode !== jsDevMode) {
    console.warn(`React was already loaded with js_dev_mode=${loadedDevMode}, so js_dev_mode=${jsDevMode} is ignored.`);
  }
  return loadedDevMode;
}

export type DepsOptions = {
  jsDevMode: boolean;
  jsPackageVersion: string;
  customJsUrl: string;
};

export async function loadDeps({ jsDevMode, jsPackageVersion, customJsUrl }: DepsOptions): Promise<PluotReactModule> {
  const loadedDevMode = await loadReact(jsDevMode);
  const pluotReactVersion = jsPackageVersion || __PLUOT_REACT_VERSION__;
  const importMap = getImportMap(loadedDevMode, pluotReactVersion);
  if (customJsUrl) {
    importMap.imports["@pluot/react"] = customJsUrl;
    return importWithMapAndFallback("@pluot/react", importMap, null);
  }
  return importWithMapAndFallback("@pluot/react", importMap, getFallbackImportMap(pluotReactVersion));
}
