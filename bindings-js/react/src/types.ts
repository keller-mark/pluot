import type { ReactElement } from "react";
import type {
  AspectRatioMode,
  AspectRatioAlignmentMode,
  CameraMatrix,
  StoreInput,
  StoresInput,
  StoresOutput,
} from "@pluot/core";

// TODO: auto-generate the types that mirror Rust structs/enums:
// https://github.com/keller-mark/pluot/issues/133

// === Plot params ===

/** Mirrors the Rust `ViewMode` enum (serde-renamed to lowercase). */
export type ViewMode = "2d" | "3d";

/** Mirrors the Rust `GraphicsFormat` enum. */
export type GraphicsFormat = "Raster" | "Vector";

/** Mirrors the adjacently-tagged Rust `PlotParams` enum discriminant. */
export type PlotType = "LayeredPlot";

/**
 * One entry of `PlotParams.layers`, mirroring the adjacently-tagged Rust
 * `LayerParams` enum: `{ "layer_type": "PointLayer", "layer_params": { ... } }`.
 *
 * `layer_type` must name a registered layer (see the `LayerParams` enum in
 * `crates/pluot/src/render_params.rs`), and the shape of `layer_params`
 * depends on which layer was named.
 */
export type LayerParams = {
  layer_type: string;
  layer_params: Record<string, unknown>;
};

/** Mirrors the Rust `LayeredPlotRenderParams` struct. */
export type PlotParams = {
  layers: LayerParams[];
};

/**
 * The object handed to the wasm `render_wasm` / `pick_wasm` functions.
 * Mirrors the Rust `RenderParams` struct (snake_case, unlike the props of
 * the {@link PluotProps} React API).
 */
export type RenderParams = {
  schema_version: string | null;
  width: number;
  height: number;
  format: GraphicsFormat;
  margin_top: number;
  margin_right: number;
  margin_bottom: number;
  margin_left: number;
  device_pixel_ratio: number;
  aspect_ratio_mode: AspectRatioMode;
  aspect_ratio_alignment_mode: AspectRatioAlignmentMode;
  view_mode: ViewMode;
  pickable: boolean;
  camera_view: CameraMatrix | null;
  plot_id: string;
  plot_type: PlotType;
  stores: StoresOutput | undefined;
  plot_params: PlotParams;
  /** In milliseconds. Has no effect when `wait_for_store_gets` is false. */
  timeout: number | null;
  wait_for_store_gets: boolean;
  cache_enabled: boolean;
  svg_compression_enabled: boolean;
  svg_include_document: boolean;
};

// === Picking results ===

/** Mirrors the Rust `ScreenCoord` struct. Y increases upwards. */
export type ScreenCoord = {
  x: number;
  y: number;
};

/** Mirrors the externally-tagged Rust `DataCoord` enum. */
export type DataCoord =
  | { TwoD: { x: number; y: number } }
  | { ThreeD: { x: number; y: number; z: number } };

/** Mirrors the Rust `LayerPickingResult` struct. */
export type LayerPickingResult = {
  layer_id: string;
  info: Record<string, string>;
};

/**
 * Mirrors the Rust `PickingResult` struct, after normalization
 * of the `info` Maps (produced by serde-wasm-bindgen) to plain objects.
 *
 * Note: `serde_wasm_bindgen` serializes a Rust `None` as `undefined`
 * (not `null`), so `data_coord` is absent rather than null when picking
 * did not resolve to a data coordinate.
 */
export type PickingResult = {
  data_coord: DataCoord | undefined;
  screen_coord: ScreenCoord;
  layer_results: LayerPickingResult[];
};

/**
 * The un-normalized shape that `pick_wasm` actually resolves to. It is typed
 * `any` on the wasm-bindgen side, so this type is what documents the wire
 * format: `serde_wasm_bindgen` converts the Rust `HashMap` behind `info` into
 * a JS `Map`, which {@link PickingResult} flattens to a plain object.
 */
export type RawLayerPickingResult = Omit<LayerPickingResult, "info"> & {
  info: Map<string, string>;
};

export type RawPickingResult = Omit<PickingResult, "layer_results"> & {
  layer_results: RawLayerPickingResult[];
};

// === Tooltip ===

/**
 * What a {@link PluotProps.onHover} callback may return for the tooltip to
 * render. A plain object is rendered as a key/value table when `asTable`
 * is set, and as pretty-printed JSON otherwise.
 */
export type TooltipContent =
  | string
  | number
  | ReactElement
  | Record<string, unknown>
  | null
  | undefined;

export type TooltipProps = {
  content: TooltipContent;
  /** Render a plain-object `content` as a two-column key/value table. */
  asTable?: boolean;
};

/** The hovered point plus the tooltip content to show for it. */
export type HoverInfo = {
  content: TooltipContent;
  /** Mouse position in the coordinate space of the outer (width x height) container. */
  mouseX: number;
  mouseY: number;
};

// === Component props ===

export type PluotProps = {
  /**
   * The schema version used to generate the plot, for forward compatibility.
   * A mismatch with the Rust crate version logs a warning.
   */
  schemaVersion?: string | null;
  /** Width of the plot, in pixels. */
  width: number;
  /** Height of the plot, in pixels. */
  height: number;
  /**
   * Unique-per-page plot ID, used to key caches of intermediate values.
   * Also the default store name when `store` is used without `storeName`.
   */
  plotId: string;
  plotType: PlotType;
  plotParams: PlotParams;

  /**
   * A single Zarr store: a URL string, a zarrita store instance,
   * or already-derived `ZarrStoreInfo` metadata.
   * Mutually exclusive with `stores`.
   */
  store?: StoreInput;
  /** The name to register `store` under. Defaults to `plotId`. */
  storeName?: string;
  /** Multiple Zarr stores, keyed by store name. Mutually exclusive with `store`. */
  stores?: StoresInput;
  /**
   * Whether to register the store(s) with the wasm module.
   * Set to false when they have already been registered elsewhere.
   */
  registerStores?: boolean;

  viewMode?: ViewMode;
  format?: GraphicsFormat;
  marginTop?: number;
  marginRight?: number;
  marginBottom?: number;
  marginLeft?: number;
  aspectRatioMode?: AspectRatioMode;
  aspectRatioAlignmentMode?: AspectRatioAlignmentMode;
  /** Outline the margin box and the plot area, to help debug layout. */
  debugMargins?: boolean;
  backgroundColor?: string;

  /** Lower bound (in ms) of the exponential backoff between bailed-early renders. */
  minTimeout?: number;
  /** Upper bound (in ms) of the exponential backoff between bailed-early renders. */
  maxTimeout?: number;
  /** Whether a new render may start while a previous one is still in flight. */
  allowSimultaneousRenders?: boolean;

  /**
   * The 4x4 camera matrix. Without `setCameraMatrix`, this is treated as the
   * initial value only, and the camera is managed internally.
   */
  cameraMatrix?: CameraMatrix | null;
  /** Provide to take control of the camera matrix. */
  setCameraMatrix?: ((cameraMatrix: CameraMatrix) => void) | null;

  /** Whether clicking should run a picking query and call `onClick`. */
  enableClick?: boolean;
  /** Whether hovering should run a picking query and show a tooltip via `onHover`. */
  enableTooltip?: boolean;
  onClick?: ((result: PickingResult) => void) | null;
  onHover?: ((result: PickingResult) => TooltipContent) | null;
};
