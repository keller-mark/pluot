"""AnyWidget that renders Pluot plots server-side and ships the bytes to the browser.

The Python kernel performs the GPU/CPU rendering via :func:`pluot.render` and sends
the resulting raw RGBA bytes back to the browser through the widget protocol's
binary buffer channel. The JS side paints those bytes onto an HTML ``<canvas>``
and uses the camera helpers from ``@pluot/core`` to translate mouse/wheel events
into camera matrix updates that are synced back to the kernel, triggering the
next render.
"""

from __future__ import annotations

import asyncio
from typing import Any

import anywidget
import traitlets

from .render import render


# Identity-with-z-scale matrix, matching the default used by the @pluot/react
# component so plots look the same regardless of which binding renders them.
DEFAULT_CAMERA_MATRIX_2D: list[float] = [
    1.0, 0.0, 0.0,       0.0,
    0.0, 1.0, 0.0,       0.0,
    0.0, 0.0, 1.0 / 200, 0.0,
    0.0, 0.0, 0.0,       1.0,
]

DEFAULT_CAMERA_MATRIX_3D: list[float] = [
    1.0,  0.0, 0.0, 0.0,
    0.0,  1.0, 0.0, 0.0,
    0.0,  0.0, 1.0, 0.0,
    0.0,  0.0, -10.0, 1.0,
]


_ESM = r"""
import * as pluot from 'https://unpkg.com/@pluot/core@0.1.1/dist/index.js';

function getViewportParams(model) {
    return {
        width: model.get("width"),
        height: model.get("height"),
        aspectRatioMode: model.get("aspect_ratio_mode"),
        aspectRatioAlignmentMode: model.get("aspect_ratio_alignment_mode"),
        margins: {
            marginTop: model.get("margin_top"),
            marginRight: model.get("margin_right"),
            marginBottom: model.get("margin_bottom"),
            marginLeft: model.get("margin_left"),
        },
    };
}

function toUint8(buf) {
    if (buf instanceof Uint8Array) return buf;
    if (ArrayBuffer.isView(buf)) {
        return new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
    }
    return new Uint8Array(buf);
}

function matricesEqual(a, b) {
    for (let i = 0; i < 16; i++) {
        if (a[i] !== b[i]) return false;
    }
    return true;
}

function initialize({ model }) {
}

function render({ model, el }) {
    const container = document.createElement("div");
    container.style.position = "relative";

    const canvas = document.createElement("canvas");
    canvas.style.display = "block";
    container.appendChild(canvas);

    // Transparent overlay that captures pointer events for the camera, sized to
    // the layer area (the canvas minus the configured margins).
    const cameraEl = document.createElement("div");
    cameraEl.style.position = "absolute";
    container.appendChild(cameraEl);

    el.appendChild(container);

    function applyLayout() {
        const w = model.get("width");
        const h = model.get("height");
        const mt = model.get("margin_top");
        const mr = model.get("margin_right");
        const mb = model.get("margin_bottom");
        const ml = model.get("margin_left");

        container.style.width = `${w}px`;
        container.style.height = `${h}px`;
        canvas.style.width = `${w}px`;
        canvas.style.height = `${h}px`;
        if (canvas.width !== w) canvas.width = w;
        if (canvas.height !== h) canvas.height = h;

        cameraEl.style.top = `${mt}px`;
        cameraEl.style.left = `${ml}px`;
        cameraEl.style.width = `${Math.max(0, w - ml - mr)}px`;
        cameraEl.style.height = `${Math.max(0, h - mt - mb)}px`;
    }
    applyLayout();

    function pickWheelHandler() {
        return model.get("view_mode") === "3d" ? pluot.onWheel3d : pluot.onWheel2d;
    }
    function pickMouseMoveHandler() {
        return model.get("view_mode") === "3d" ? pluot.onMouseMove3d : pluot.onMouseMove2d;
    }

    // Coalesce rapid camera updates (mousemove fires faster than rAF rate) into
    // one model.set per animation frame.  _pendingMatrix also serves as the base
    // for the next computation so transforms accumulate correctly even when the
    // previous update hasn't been flushed to the model yet.
    let _rafId = null;
    let _pendingMatrix = null;

    function scheduleMatrixUpdate(next) {
        _pendingMatrix = next;
        if (_rafId === null) {
            _rafId = requestAnimationFrame(() => {
                _rafId = null;
                if (_pendingMatrix !== null) {
                    model.set("camera_matrix", Array.from(_pendingMatrix));
                    model.save_changes();
                    _pendingMatrix = null;
                }
            });
        }
    }

    function onWheel(event) {
        const cur = new Float32Array(_pendingMatrix || model.get("camera_matrix"));
        const next = pickWheelHandler()(getViewportParams(model), cur, event);
        if (matricesEqual(cur, next)) return;
        scheduleMatrixUpdate(next);
    }

    function onMouseMove(event) {
        const cur = new Float32Array(_pendingMatrix || model.get("camera_matrix"));
        const next = pickMouseMoveHandler()(getViewportParams(model), cur, event);
        if (matricesEqual(cur, next)) return;
        scheduleMatrixUpdate(next);
    }

    cameraEl.addEventListener("wheel", onWheel, { passive: false });
    cameraEl.addEventListener("mousemove", onMouseMove);

    function onCustomMsg(msg, buffers) {
        if (!msg || msg.kind !== "render-result") return;
        if (!buffers || !buffers[0]) return;

        const w = model.get("width");
        const h = model.get("height");
        const u8 = toUint8(buffers[0]);
        if (u8.length !== w * h * 4) {
            // Stale frame for a previous size; drop it and wait for the next.
            return;
        }
        const ctx = canvas.getContext("2d");
        if (!ctx) return;
        const clamped = new Uint8ClampedArray(u8.buffer, u8.byteOffset, u8.byteLength);
        ctx.putImageData(new ImageData(clamped, w, h), 0, 0);
    }
    model.on("msg:custom", onCustomMsg);

    const layoutKeys = [
        "width", "height",
        "margin_top", "margin_right", "margin_bottom", "margin_left",
    ];
    for (const key of layoutKeys) {
        model.on(`change:${key}`, applyLayout);
    }

    // Ask the kernel for the initial frame now that the widget is mounted.
    model.send({ kind: "ready" });

    return () => {
        if (_rafId !== null) cancelAnimationFrame(_rafId);
        cameraEl.removeEventListener("wheel", onWheel);
        cameraEl.removeEventListener("mousemove", onMouseMove);
        model.off("msg:custom", onCustomMsg);
        for (const key of layoutKeys) {
            model.off(`change:${key}`, applyLayout);
        }
    };
}

export default { initialize, render };
"""


_RENDER_TRIGGER_TRAITS = (
    "camera_matrix",
    "width",
    "height",
    "margin_top",
    "margin_right",
    "margin_bottom",
    "margin_left",
    "aspect_ratio_mode",
    "aspect_ratio_alignment_mode",
    "view_mode",
    "plot_id",
    "plot_type",
    "store_name",
    "plot_params",
    "format",
)


class PluotPyWidget(anywidget.AnyWidget):
    """AnyWidget that renders a Pluot plot in the kernel and paints to a canvas."""

    _esm = _ESM

    # Synced traits: JS needs these to compute camera updates and lay out the canvas.
    width = traitlets.Int(800).tag(sync=True)
    height = traitlets.Int(800).tag(sync=True)
    camera_matrix = traitlets.List(
        trait=traitlets.Float(),
        default_value=DEFAULT_CAMERA_MATRIX_2D,
    ).tag(sync=True)
    margin_top = traitlets.Float(0.0).tag(sync=True)
    margin_right = traitlets.Float(0.0).tag(sync=True)
    margin_bottom = traitlets.Float(0.0).tag(sync=True)
    margin_left = traitlets.Float(0.0).tag(sync=True)
    aspect_ratio_mode = traitlets.Unicode("Contain").tag(sync=True)
    aspect_ratio_alignment_mode = traitlets.Unicode("Center").tag(sync=True)
    view_mode = traitlets.Unicode("2d").tag(sync=True)

    # Python-only traits: passed straight to `render()`, not needed on the JS side.
    plot_id = traitlets.Unicode("plot")
    plot_type = traitlets.Unicode("LayeredPlot")
    store_name = traitlets.Unicode("")
    plot_params = traitlets.Dict(default_value={})
    format = traitlets.Unicode("Raster")

    def __init__(self, **kwargs: Any) -> None:
        self._render_inflight = False
        self._render_pending = False
        super().__init__(**kwargs)
        self.on_msg(self._handle_msg)

    def _handle_msg(self, _widget: Any, content: Any, _buffers: list[bytes]) -> None:
        if isinstance(content, dict) and content.get("kind") == "ready":
            self._schedule_render()

    @traitlets.observe(*_RENDER_TRIGGER_TRAITS)
    def _on_trait_change(self, _change: dict) -> None:
        self._schedule_render()

    def _schedule_render(self) -> None:
        if self._render_inflight:
            self._render_pending = True
            return
        try:
            loop = asyncio.get_event_loop()
        except RuntimeError:
            return
        if loop.is_running():
            loop.create_task(self._render_loop())
        else:
            loop.run_until_complete(self._render_loop())

    async def _render_loop(self) -> None:
        # Coalesce concurrent triggers: while a render is in flight, set a pending
        # flag and let the running task pick up the latest state when it finishes.
        if self._render_inflight:
            self._render_pending = True
            return
        self._render_inflight = True
        try:
            while True:
                self._render_pending = False
                await self._render_once()
                if not self._render_pending:
                    return
        finally:
            self._render_inflight = False

    async def _render_once(self) -> None:
        try:
            result = await render(
                width=self.width,
                height=self.height,
                plot_id=self.plot_id,
                plot_type=self.plot_type,
                store_name=self.store_name,
                plot_params=self.plot_params,
                camera_view=list(self.camera_matrix),
                margin_top=self.margin_top,
                margin_right=self.margin_right,
                margin_bottom=self.margin_bottom,
                margin_left=self.margin_left,
                aspect_ratio_mode=self.aspect_ratio_mode,
                aspect_ratio_alignment_mode=self.aspect_ratio_alignment_mode,
                view_mode=self.view_mode,
                format=self.format,
            )
        except Exception as exc:  # noqa: BLE001 - surface to the browser for debugging
            self.send({"kind": "render-error", "error": repr(exc)})
            return

        # `render()` appends one trailing byte (the bailed-early flag); strip it
        # before sending the raw RGBA bytes over the buffer channel.
        image_bytes = bytes(result[:-1])
        self.send({"kind": "render-result"}, buffers=[image_bytes])
