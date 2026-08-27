import React, { useEffect, useEffectEvent, useMemo, useRef } from "react";
import { brush as d3Brush, brushX as d3BrushX, brushY as d3BrushY } from "d3-brush";
import { select } from "d3-selection";
import { isEqual } from "lodash-es";

// The d3-brush default handle size, in pixels.
const DEFAULT_HANDLE_SIZE = 6;

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

// Computes the brushable region as a d3-brush extent
// ([[x0, y0], [x1, y1]], in pixels relative to the top-left of the
// width x height container).
// The margin regions are constrained to the layer bounds along their
// cross-axis, so that (for example) a "marginBottom" brush lines up with the
// plotted region rather than extending into the bottom-left/right corners.
function getExtent(brushRegion, width, height, marginLeft, marginRight, marginTop, marginBottom) {
  switch (brushRegion) {
    case "full":
      return [[0, 0], [width, height]];
    case "marginLeft":
      return [[0, marginTop], [marginLeft, height - marginBottom]];
    case "marginRight":
      return [[width - marginRight, marginTop], [width, height - marginBottom]];
    case "marginTop":
      return [[marginLeft, 0], [width - marginRight, marginTop]];
    case "marginBottom":
      return [[marginLeft, height - marginBottom], [width - marginRight, height]];
    case "layer":
    default:
      return [[marginLeft, marginTop], [width - marginRight, height - marginBottom]];
  }
}

// Converts a d3-brush selection to the rectangle that we emit to the parent.
// For 1D brushes, the un-brushed dimension spans the full brushable region.
function selectionToRect(selection, brushType, extent) {
  if (!selection) {
    return null;
  }
  let x0;
  let x1;
  let y0;
  let y1;
  if (brushType === "x") {
    [x0, x1] = selection;
    [y0, y1] = [extent[0][1], extent[1][1]];
  } else if (brushType === "y") {
    [y0, y1] = selection;
    [x0, x1] = [extent[0][0], extent[1][0]];
  } else {
    [[x0, y0], [x1, y1]] = selection;
  }
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0, x0, y0, x1, y1 };
}

// The inverse of selectionToRect. Accepts either the { x, y, width, height }
// or the { x0, y0, x1, y1 } form of the rectangle.
function rectToSelection(rect, brushType) {
  if (!rect) {
    return null;
  }
  const x0 = rect.x0 ?? rect.x;
  const y0 = rect.y0 ?? rect.y;
  const x1 = rect.x1 ?? rect.x + rect.width;
  const y1 = rect.y1 ?? rect.y + rect.height;
  if (brushType === "x") {
    return [x0, x1];
  }
  if (brushType === "y") {
    return [y0, y1];
  }
  return [[x0, y0], [x1, y1]];
}

// Clamps a selection to the brushable region, returning null for selections
// that are degenerate (zero-size), non-numeric, or entirely outside of it.
function clampSelection(selection, brushType, extent) {
  if (!selection) {
    return null;
  }
  const [[extentX0, extentY0], [extentX1, extentY1]] = extent;

  const clampInterval = (lo, hi, min, max) => {
    const start = clamp(Math.min(lo, hi), min, max);
    const end = clamp(Math.max(lo, hi), min, max);
    // NaN comparisons are false, so this also rejects non-numeric input.
    return end > start ? [start, end] : null;
  };

  if (brushType === "x") {
    return clampInterval(selection[0], selection[1], extentX0, extentX1);
  }
  if (brushType === "y") {
    return clampInterval(selection[0], selection[1], extentY0, extentY1);
  }
  const xs = clampInterval(selection[0][0], selection[1][0], extentX0, extentX1);
  const ys = clampInterval(selection[0][1], selection[1][1], extentY0, extentY1);
  if (!xs || !ys) {
    return null;
  }
  return [[xs[0], ys[0]], [xs[1], ys[1]]];
}

/**
 * A click-and-drag brush overlay, backed by d3-brush.
 *
 * Rendered as an absolutely-positioned SVG that covers the full width x height
 * of its (position: relative) parent, so that it can be layered on top of a
 * Pluot component that was given the same width/height/margin props.
 * Pointer events only land on the brushable region; everywhere else they fall
 * through to the elements below (e.g. the Pluot camera element).
 *
 * Props:
 * - width, height: the size of the container, in pixels.
 * - marginLeft, marginRight, marginTop, marginBottom: the plot margins, in
 *   pixels. Should match the values passed to the Pluot component.
 * - brushType: "x" or "y" for a 1D brush, "xy" for a 2D (rectangular) brush.
 * - brushRegion: where brushing is allowed. One of "full" (the entire
 *   width x height), "layer" (the plotted region within the margins), or
 *   "marginLeft"/"marginRight"/"marginTop"/"marginBottom" (a single margin).
 * - onBrush: (rect, info) => void, called as the user brushes. `rect` is
 *   { x, y, width, height, x0, y0, x1, y1 } in pixels relative to the
 *   top-left of the container, or null when the selection is cleared.
 *   `info` is { type, sourceEvent }, where type is "start" | "brush" | "end".
 * - emitDuringDrag: when false, onBrush is only called on "end" events.
 * - selection: an optional controlled selection, in the same shape as `rect`
 *   (or null for no selection). When omitted, the brush is uncontrolled and
 *   d3-brush owns the selection.
 * - enabled: when false, new brush gestures are ignored, but any existing
 *   selection remains visible.
 * - handleSize, className, style, debugRegion: presentational escape hatches.
 */
export function Brush(props) {
  const {
    width,
    height,
    marginLeft = 100.0,
    marginRight = 100.0,
    marginTop = 100.0,
    marginBottom = 100.0,
    brushType = "xy", // "x", "y", "xy"
    brushRegion = "layer", // "full", "layer", "marginLeft", "marginRight", "marginTop", "marginBottom"
    onBrush: onBrushProp = null,
    emitDuringDrag = true,
    selection: controlledSelection,
    enabled = true,
    handleSize = DEFAULT_HANDLE_SIZE,
    className = "pluot-brush",
    style = null,
    debugRegion = false,
  } = props;

  // If the selection prop is omitted entirely, the brush is uncontrolled.
  // (Passing null means "controlled, with nothing currently selected".)
  const isControlled = controlledSelection !== undefined;

  const gRef = useRef(null);
  // The d3-brush behavior currently attached to the <g> element.
  const brushRef = useRef(null);
  // The selection currently applied to the DOM, in d3-brush's own format.
  // Kept in a ref rather than in state because d3 owns the rendered rect;
  // we only need it to restore the selection when the brush is re-created.
  const appliedSelectionRef = useRef(null);

  const extent = useMemo(
    () => getExtent(brushRegion, width, height, marginLeft, marginRight, marginTop, marginBottom),
    [brushRegion, width, height, marginLeft, marginRight, marginTop, marginBottom],
  );

  // A zero-size (or inverted) region would leave d3-brush with nothing to
  // attach its overlay to, e.g. a "marginTop" region when marginTop is 0.
  const isExtentValid = extent[1][0] > extent[0][0] && extent[1][1] > extent[0][1];

  // These are wrapped in useEffectEvent so that they always see the latest
  // props without the brush having to be torn down and re-created whenever
  // a callback identity or an unrelated prop changes.
  const emit = useEffectEvent((selection, type, sourceEvent) => {
    if (typeof onBrushProp !== "function" || (!emitDuringDrag && type !== "end")) {
      return;
    }
    onBrushProp(selectionToRect(selection, brushType, extent), { type, sourceEvent });
  });

  // The d3-brush default filter, plus the `enabled` prop.
  const brushFilter = useEffectEvent((event) => enabled && !event.ctrlKey && !event.button);

  const handleBrushEvent = useEffectEvent((event) => {
    appliedSelectionRef.current = event.selection ?? null;
    // Programmatic brush.move/brush.clear calls have no sourceEvent; those are
    // emitted (or deliberately not emitted) by applySelection below, so that
    // syncing the controlled `selection` prop cannot feed back into the parent.
    if (event.sourceEvent) {
      emit(event.selection ?? null, event.type, event.sourceEvent);
    }
  });

  // Applies a selection to the DOM, clamped to the current brushable region.
  // `force` re-applies even if the value is unchanged, which is needed after
  // the brush has been re-created and its DOM rebuilt from scratch.
  const applySelection = useEffectEvent((requested, force = false) => {
    const g = gRef.current;
    const brush = brushRef.current;
    if (!g || !brush) {
      return;
    }
    const next = clampSelection(requested, brushType, extent);
    // Skipping no-op updates is what keeps a controlled parent from fighting
    // d3 mid-drag: it echoes each "brush" event back to us via the selection
    // prop, and re-applying that value would restart the gesture.
    if (!force && isEqual(next, appliedSelectionRef.current)) {
      return;
    }
    appliedSelectionRef.current = next;
    const gSel = select(g);
    if (next === null) {
      gSel.call(brush.clear);
    } else {
      gSel.call(brush.move, next);
    }
    // If clamping changed the selection (e.g. the container was resized out
    // from under it), tell the parent, since what it asked for is not what is
    // now displayed.
    if (!isEqual(next, requested)) {
      emit(next, "end", null);
    }
  });

  const syncSelection = useEffectEvent((force) => {
    applySelection(
      isControlled ? rectToSelection(controlledSelection, brushType) : appliedSelectionRef.current,
      force,
    );
  });

  // Create and attach the d3-brush behavior. Re-runs only when the geometry of
  // the brush itself changes, not when the selection changes.
  useEffect(() => {
    const g = gRef.current;
    if (!g || !isExtentValid) {
      return () => {};
    }

    const createBrush = brushType === "x" ? d3BrushX : brushType === "y" ? d3BrushY : d3Brush;
    const brush = createBrush()
      .extent(extent)
      .handleSize(handleSize)
      .filter(brushFilter)
      .on("start brush end", handleBrushEvent);

    select(g).call(brush);
    brushRef.current = brush;

    // Restore the selection into the freshly-created overlay/handle elements.
    syncSelection(true);

    return () => {
      // Detach d3's listeners and remove the elements it created, so that
      // React is handed back an empty <g>.
      brush.on("start brush end", null);
      select(g).on(".brush", null).selectAll("*").remove();
      // d3-brush keeps its state (including the selection) on the node, and
      // re-uses it if the node is re-initialized. Drop it so that the effect
      // above stays the only thing that restores a selection.
      delete g.__brush;
      brushRef.current = null;
    };
  }, [brushType, extent, handleSize, isExtentValid]);

  // Sync the controlled selection prop into the DOM. Deliberately separate
  // from the effect above so that a parent updating `selection` during a drag
  // does not tear down the in-progress brush.
  useEffect(() => {
    if (isControlled) {
      syncSelection(false);
    }
  }, [isControlled, controlledSelection]);

  if (!(width > 0 && height > 0)) {
    return null;
  }

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      xmlns="http://www.w3.org/2000/svg"
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        width,
        height,
        // The brush elements below opt back in, so that only the brushable
        // region intercepts pointer events.
        pointerEvents: "none",
        ...style,
      }}
    >
      {debugRegion && isExtentValid ? (
        <rect
          x={extent[0][0]}
          y={extent[0][1]}
          width={extent[1][0] - extent[0][0]}
          height={extent[1][1] - extent[0][1]}
          fill="none"
          stroke="blue"
          strokeDasharray="4 2"
        />
      ) : null}
      {/* d3-brush renders its overlay, selection, and handle rects into this
          group. pointerEvents is set as an attribute rather than as inline
          style because d3-brush toggles the same attribute while dragging. */}
      <g ref={gRef} className={className} pointerEvents="all" />
    </svg>
  );
}
