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

/** Mirrors the Rust `LayerBrushingResult` struct. */
export type LayerBrushingResult = {
  layer_id: string;
  info: Record<string, string>;
  element_info: Record<string, string[]>;
};

/**
 * Mirrors the Rust `BrushingResult` struct, after normalization of the `info`
 * and `element_info` maps (produced by serde-wasm-bindgen) to plain objects.
 */
export type BrushingResult = {
  layer_results: LayerBrushingResult[];
};

/**
 * The un-normalized shape that `brush_wasm` actually resolves to. It is typed
 * `any` on the wasm-bindgen side, so this type is what documents the wire
 * format: `serde_wasm_bindgen` converts the Rust `HashMap`s behind `info` and
 * `element_info` into JS `Map`s, which {@link BrushingResult} flattens to
 * plain objects.
 */
export type RawLayerBrushingResult = Omit<LayerBrushingResult, "info" | "element_info"> & {
  info: Map<string, string>;
  element_info: Map<string, string[]>;
};

export type RawBrushingResult = Omit<BrushingResult, "layer_results"> & {
  layer_results: RawLayerBrushingResult[];
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

// === Brushing ===

/**
 * Which representation of a {@link BrushVertex} is authoritative for an axis.
 *
 * - `Pixels`: relative to the top-left of the outer (width x height) container,
 *   with Y increasing downwards (the DOM/SVG convention). Unaffected by the camera.
 * - `Data`: the data coordinate under the current camera, as reported by
 *   `getBounds`, with Y increasing upwards. A brush in this mode is pinned to the
 *   data, so it moves on screen as the user zooms/pans.
 * - `Normalized`: a 0-to-1 fraction of the brushable region, with Y increasing
 *   upwards (0 at the bottom edge, 1 at the top edge). Unaffected by the camera.
 */
export type BrushUnitsMode = "Pixels" | "Data" | "Normalized";

// For each brushed rect/polygon vertex,
// we represent it using all units modes simultaneously.
// Only the representation matching `brushUnitsModeX`/`brushUnitsModeY` is
// authoritative; the other two are derived from it and are recomputed whenever
// the camera, the container size, or the margins change.
export type BrushVertex = {
  // Data unitsMode.
  x_data: number,
  y_data: number,
  // Pixels unitsMode.
  x_pixels: number,
  y_pixels: number,
  // Normalized unitsMode.
  x_normalized: number,
  y_normalized: number,
};

/**
 * The shape the user draws, and which the resulting {@link BrushState} holds.
 *
 * - `Rect`: click and drag to draw a rectangle.
 * - `Polygon`: click and drag to draw a lasso, defining vertices as the user
 *   drags. The number of vertices is limited by using lodash-es throttle.
 * - `RangeX`: select a horizontal range. The overlay renders as a rectangle
 *   which takes up the full brush height, according to the brush margins.
 * - `RangeY`: select a vertical range. The overlay renders as a rectangle
 *   which takes up the full brush width, according to the brush margins.
 */
export type BrushMode = 'Rect' | 'Polygon' | 'RangeX' | 'RangeY';

/** The axis-aligned modes, all of which are stored as four rectangle corners. */
export type RectLikeBrushMode = Exclude<BrushMode, 'Polygon'>;

export type BrushState = {
  // Is the user still drawing, or have they completed their drag interaction?
  status: 'Drawing' | 'Complete';
  shape: BrushMode,
  // For every shape but Polygon, always four corners ordered clockwise in pixel
  // space starting from the top-left, so corner `i` is diagonally opposite
  // corner `(i + 2) % 4`.
  // For RangeX and RangeY, the axis that is not being selected always spans the
  // full brushable extent, so it is re-pinned whenever that extent changes.
  vertices: BrushVertex[],
};

/**
 * The value of {@link PluotProps.brush} meaning "controlled, but nothing is
 * brushed right now".
 *
 * `undefined` cannot play this role: a prop that was never passed is
 * indistinguishable from one explicitly set to `undefined`, and an absent
 * `brush` has to mean uncontrolled. A parent that controls the brush therefore
 * passes `NO_BRUSH` rather than `undefined` to show no brush, which keeps it
 * controlled across the empty state instead of silently handing control back.
 */
export const NO_BRUSH = "NoBrush";
export type NoBrush = typeof NO_BRUSH;

// TODO: On the rust side, define a Brushable.brush trait, analogous to Pickable.pick.
export type BrushResult = {
  // Similar to picking, upon brush, the Rust side can return a per-layer Map with essentially any data
  // (such as the list of entity IDs within the brushed region).
  // The rust side can also return a new rect/polygon to "snap"/quantize to.
  // TODO: fill in the rest of this struct.
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

  // Brushing supports both a rectangular brush and a lasso (i.e., polygonal) brush.
  // We draw a brush overlay as an SVG to indicate the drawn rect/polygon (both during the draw interactions and following completion).
  // The brush overlay consists of either a rectangle with circle elements at its corner vertices,
  // or a circle element at each polygon vertex, with lines connecting the polygon vertices.

  // When the brush units mode is "Data", the brushed overlay rect/polygon is dependent on the camera matrix and responds to camera state updates.
  // As the user zooms/pans, the overlay updates if the unitsMode is "Data" in either the X, Y, or XY directions.
  // Both default to "Data".
  brushUnitsModeX?: BrushUnitsMode;
  brushUnitsModeY?: BrushUnitsMode;

  // The brush margins restrict the brushable region to within the specified brush bounds.
  // Each defaults to the corresponding layer margin, so by default the brushable region is the layer region.
  // However, when brushUnitsModeY is "Data", we ignore brushMarginTop and brushMarginBottom, and instead the layer (i.e., camera) bounds (marginTop and marginBottom) take precedence.
  brushMarginTop?: number;
  brushMarginBottom?: number;
  // However, when brushUnitsModeX is "Data", we ignore brushMarginLeft and brushMarginRight, and instead the layer (i.e., camera) bounds (marginLeft and marginRight) take precedence.
  brushMarginLeft?: number;
  brushMarginRight?: number;

  // When true, the user can draw a brush rect/polygon by long-clicking and then dragging.
  enableBrushCreate?: boolean;
  // When true, the user can modify the vertices of persisted brushes (uncontrolled) or brushes passed via `brush` prop (controlled) by interacting with the overlay.
  // For Rect, RangeX and RangeY, the user can also drag a side of the overlay to extend the brush in that direction alone.
  // A range brush only exposes the two sides on the axis it selects, since the other axis always spans the whole brushable region.
  enableBrushEdit?: boolean;
  // When true, we display a clear button upon hovering the brush rect/polygon, to allow the user to clear/cancel the brush.
  enableBrushClear?: boolean;

  // Long-click of 1.5s to trigger a brushing interaction. If the user long-clicks for this amount of milliseconds, then they can being drawing the brush rect/lasso.
  // Only relevant when enableBrushCreate is true.
  // By default, 1500 ms.
  brushDelay?: number;

  // When a user has begun to click-and-hold for this amount of ms, we render a small circle at the current mouse cursor position, and animate the circle "filling" by rendering a wedge (slice of pie) with a larger angle until the wedge fills the whole pie (finishing at the specified brushDelay duration).
  // Only relevant when enableBrushCreate is true.
  // By default, 250ms.
  maybeBrushDelay?: number;

  // If true, the brush overlay should remain after the drag interaction.
  // If false, the brush overlay should be removed upon the end of the drag interaction, after calling onBrushEnd.
  persistBrush?: boolean;

  // Which shape the user draws. By default, "Rect".
  brushMode?: BrushMode;

  // Color of the brush overlay (outline and vertex/edge handles). The fill uses
  // this same color at reduced opacity. By default, "#3b6ea5".
  brushColor?: string;

  // For brushing, we support both controlled and uncontrolled (similar to the cameraMatrix/setCameraMatrix).
  // When controlled, the parent provides the brush state (rect/polygon vertices) or `NO_BRUSH`.
  // When uncontrolled, the value of `brush` is `null` (or the prop is omitted), so the brush state will be managed internally.
  // When controlled via parent, we ignore the persistBrush prop; instead, the brush persists while the BrushState is specified/present.
  // If null or absent, we take this to mean uncontrolled.
  // If a BrushState object or `NO_BRUSH` is provided, we take this to mean controlled.
  // A controlled parent must use `NO_BRUSH` rather than `undefined` for the empty
  // state, since `undefined` is indistinguishable from the prop being omitted and
  // would hand control back mid-interaction, resurfacing whatever the internal
  // (uncontrolled) state last held.
  // Note that when controlled, enableBrushCreate can be false (the user cannot long-click to draw a new brush),
  // but the parent may still provide a brush value.
  // When controlled, we only emit onBrush/onBrushEnd for internally-triggered updates (e.g., if enableBrushEdit is true) or clearing (e.g., if enableBrushClear is true).
  brush?: BrushState | NoBrush | null;

  // Called on drag interactions, as the user is drawing the brush rect/polygon.
  // Also called if the brushed rect/polygon is edited (e.g., by dragging a vertex of a persisted brush, if enableBrushEdit is true).
  // `brushingResult` is the result of running the brush query (via `brush_wasm`)
  // against the current `state`.
  onBrush?: (state: BrushState, brushingResult: BrushingResult) => BrushResult,
  // Called at the conclusion of the drag interaction, with the final (i.e., complete) brush rect/polygon.
  onBrushEnd?: (state: BrushState, brushingResult: BrushingResult) => BrushResult,

  // Called upon the user cancelling the brush, e.g., by clicking a clear button which appears when hovering the drawn rect/polygon.
  onBrushClear?: (state: BrushState) => void,

};
