"""AnyWidget that renders Pluot plots client-side via the WASM bindings.

The browser loads the Pluot WASM module and calls ``render_wasm`` directly.
Zarr data is served by the Python kernel via a custom message protocol: the JS
side sends ``anywidget-command`` messages, the Python handler fetches the
requested bytes from the registered zarr stores, and replies with
``anywidget-command-response`` messages carrying the raw bytes as binary buffers.
"""

from __future__ import annotations

import uuid
from typing import Any

import anywidget
import traitlets
from zarr.abc.store import RangeByteRequest, Store, SuffixByteRequest
from zarr.core.buffer.core import default_buffer_prototype
from .sync_store import SyncStoreWrapper


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
import * as pluot from 'https://unpkg.com/@pluot/core@0.1.2/dist/index.js';
import * as uuid from "https://esm.sh/@lukeed/uuid@2.0.1/es2022/uuid.mjs";

// Fallback UUID for non-secure (http://) contexts where crypto.randomUUID is unavailable.
function generateId() {
    return uuid.v4();
}

// Custom invoke matching the anywidget-command protocol implemented on the Python side.
function invoke(model, name, msg, buffers, signal) {
    const id = generateId();
    const abortSignal = signal ?? AbortSignal.timeout(30000);
    return new Promise((resolve, reject) => {
        if (abortSignal.aborted) { reject(abortSignal.reason); return; }
        abortSignal.addEventListener("abort", () => {
            model.off("msg:custom", handler);
            reject(abortSignal.reason);
        });
        function handler(responseMsg, responseBuffers) {
            if (!responseMsg || responseMsg.id !== id) return;
            model.off("msg:custom", handler);
            resolve([responseMsg.response, responseBuffers]);
        }
        model.on("msg:custom", handler);
        model.send({ id, kind: "anywidget-command", name, msg }, undefined, buffers ?? []);
    });
}

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

async function initialize({ model }) {
    await pluot.initialize();
}

function render({ model, el }) {
    const container = document.createElement("div");
    container.style.position = "relative";

    const canvas = document.createElement("canvas");
    canvas.style.display = "block";
    container.appendChild(canvas);

    // Transparent overlay that captures pointer events, sized to the layer area.
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

    // --- Zarr request helpers ---

    function bufferFromResponse(bufferData) {
        if (ArrayBuffer.isView(bufferData)) {
            return new Uint8Array(bufferData.buffer, bufferData.byteOffset, bufferData.byteLength);
        }
        return new Uint8Array(bufferData.buffer);
    }

    // --- Batched zarr request queue (opt-in via batch_zarr_gets traitlet) ---

    let pending = [];
    let batchId = 0;

    async function processBatch(prevPendingArr) {
        try {
            const [dataArr, buffersArr] = await invoke(
                model,
                "_zarr_get_multi",
                prevPendingArr.map(d => d.params),
            );
            prevPendingArr.forEach((item, i) => {
                const data = dataArr[i];
                if (!data.success) { item.resolve(undefined); return; }
                item.resolve(bufferFromResponse(buffersArr[i]));
            });
        } catch (err) {
            prevPendingArr.forEach(item => item.reject(err));
        }
    }

    function run() {
        const prevPending = pending;
        pending = [];
        batchId = 0;
        processBatch(prevPending);
    }

    function enqueue(params) {
        batchId = batchId || requestAnimationFrame(() => run());
        const { promise, resolve, reject } = Promise.withResolvers();
        pending.push({ params, resolve, reject });
        return promise;
    }

    // AsyncReadable store that proxies all reads back to the Python kernel.
    // When batch_zarr_gets is true, reads are coalesced into _zarr_get_multi
    // calls via requestAnimationFrame.  Otherwise each read is sent immediately
    // as an individual _zarr_get / _zarr_get_range message.
    function makeStore(storeName) {
        return {
            async get(key) {
                if (model.get("batch_zarr_gets")) {
                    return enqueue([storeName, key]);
                }
                const [data, buffers] = await invoke(model, "_zarr_get", [storeName, key]);
                if (!data.success) return undefined;
                return bufferFromResponse(buffers[0]);
            },
            async getRange(key, rangeQuery) {
                if (model.get("batch_zarr_gets")) {
                    return enqueue([storeName, key, rangeQuery]);
                }
                const [data, buffers] = await invoke(model, "_zarr_get_range", [storeName, key, rangeQuery]);
                if (!data.success) return undefined;
                return bufferFromResponse(buffers[0]);
            },
        };
    }

    // --- Rendering ---

    let renderFrame = 0;

    function scheduleRender() {
        if (renderFrame) return;
        renderFrame = requestAnimationFrame(() => {
            renderFrame = 0;
            doRender();
        });
    }

    async function doRender() {
        const w = model.get("width");
        const h = model.get("height");
        const params = {
            width: w,
            height: h,
            plot_id: model.get("plot_id"),
            plot_type: model.get("plot_type"),
            store_name: model.get("store_name"),
            plot_params: model.get("plot_params"),
            camera_view: model.get("camera_view"),
            margin_top: model.get("margin_top"),
            margin_right: model.get("margin_right"),
            margin_bottom: model.get("margin_bottom"),
            margin_left: model.get("margin_left"),
            aspect_ratio_mode: model.get("aspect_ratio_mode"),
            aspect_ratio_alignment_mode: model.get("aspect_ratio_alignment_mode"),
            view_mode: model.get("view_mode"),
            format: model.get("format"),
            device_pixel_ratio: window.devicePixelRatio,
            pickable: false,
            timeout: null,
            cache_enabled: true,
            wait_for_store_gets: true,
            svg_compression_enabled: false,
            svg_include_document: false,
        };
        try {
            const result = await pluot.render_wasm(params);
            // Strip the trailing bailed_early flag byte appended by the Rust renderer.
            const pixelCount = w * h * 4;
            const data = result.length > pixelCount ? result.subarray(0, pixelCount) : result;
            if (data.length !== pixelCount) return;
            const ctx = canvas.getContext("2d");
            if (!ctx) return;
            const clamped = new Uint8ClampedArray(data.buffer, data.byteOffset, data.byteLength);
            ctx.putImageData(new ImageData(clamped, w, h), 0, 0);
        } catch (err) {
            console.error("pluot render_wasm error:", err);
        }
    }

    // --- Camera event handlers ---

    function onWheel(event) {
        event.preventDefault();
        const cur = new Float32Array(model.get("camera_view"));
        const handler = model.get("view_mode") === "3d" ? pluot.onWheel3d : pluot.onWheel2d;
        const next = handler(getViewportParams(model), cur, event);
        if (next === cur) return;
        model.set("camera_view", Array.from(next));
        model.save_changes();
        scheduleRender();
    }

    function onMouseMove(event) {
        const cur = new Float32Array(model.get("camera_view"));
        const handler = model.get("view_mode") === "3d" ? pluot.onMouseMove3d : pluot.onMouseMove2d;
        const next = handler(getViewportParams(model), cur, event);
        if (next === cur) return;
        model.set("camera_view", Array.from(next));
        model.save_changes();
        scheduleRender();
    }

    cameraEl.addEventListener("wheel", onWheel, { passive: false });
    cameraEl.addEventListener("mousemove", onMouseMove);

    // --- Trait observers ---

    const layoutKeys = [
        "width", "height",
        "margin_top", "margin_right", "margin_bottom", "margin_left",
    ];
    for (const key of layoutKeys) {
        model.on(`change:${key}`, applyLayout);
    }

    const renderKeys = [
        "width", "height",
        "margin_top", "margin_right", "margin_bottom", "margin_left",
        "aspect_ratio_mode", "aspect_ratio_alignment_mode",
        "view_mode", "camera_view",
        "plot_id", "plot_type", "store_name", "plot_params", "format",
    ];
    for (const key of renderKeys) {
        model.on(`change:${key}`, scheduleRender);
    }

    function registerStore() {
        const storeName = model.get("store_name");
        if (storeName) {
            pluot.setStoreByName(storeName, makeStore(storeName));
        }
    }

    model.on("change:store_name", () => {
        registerStore();
        scheduleRender();
    });

    // WASM is already initialized (awaited in `initialize`); register store and render.
    registerStore();
    scheduleRender();

    return () => {
        cameraEl.removeEventListener("wheel", onWheel);
        cameraEl.removeEventListener("mousemove", onMouseMove);
        for (const key of layoutKeys) {
            model.off(`change:${key}`, applyLayout);
        }
        for (const key of renderKeys) {
            model.off(`change:${key}`, scheduleRender);
        }
        model.off("change:store_name");
        if (renderFrame) {
            cancelAnimationFrame(renderFrame);
            renderFrame = 0;
        }
    };
}

export default { initialize, render };
"""


class PluotWasmWidget(anywidget.AnyWidget):
    """AnyWidget that renders a Pluot plot client-side using the WASM bindings."""

    _esm = _ESM

    # Synced: layout and camera state.
    width = traitlets.Int(800).tag(sync=True)
    height = traitlets.Int(800).tag(sync=True)
    camera_view = traitlets.List(
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

    # Synced: plot config forwarded to render_wasm.
    plot_id = traitlets.Unicode("plot").tag(sync=True)
    plot_type = traitlets.Unicode("LayeredPlot").tag(sync=True)
    store_name = traitlets.Unicode("").tag(sync=True)
    plot_params = traitlets.Dict(default_value={}).tag(sync=True)
    format = traitlets.Unicode("Raster").tag(sync=True)
    batch_zarr_gets = traitlets.Bool(False).tag(sync=True)

    def __init__(self, stores: dict | None = None, store: Store | None = None, **kwargs: Any) -> None:
        super().__init__(**kwargs)
        self._stores: dict = dict(stores or {})
        if store is not None:
            store_name = kwargs.get("store_name") if "store_name" in kwargs else str(id(store))
            self._stores[store_name] = store
            self.store_name = store_name
        self.on_msg(self._handle_msg)

    def add_store(self, name: str, store: Any) -> None:
        self._stores[name] = store

    def _handle_msg(self, _widget: Any, content: Any, buffers: list[bytes]) -> None:
        if not isinstance(content, dict) or content.get("kind") != "anywidget-command":
            return
        self._dispatch_command(content, buffers)

    def _dispatch_command(self, msg: dict, buffers: list[bytes]) -> None:
        name = msg.get("name")
        params = msg.get("msg")
        msg_id = msg.get("id")
        try:
            if name == "_zarr_get":
                response, result_buffers = self._zarr_get(params, buffers)
            elif name == "_zarr_get_range":
                response, result_buffers = self._zarr_get_range(params, buffers)
            elif name == "_zarr_get_multi":
                response, result_buffers = self._zarr_get_multi(params, buffers)
            else:
                return
        except Exception as exc:  # noqa: BLE001
            self.send(
                {"id": msg_id, "kind": "anywidget-command-response", "response": {"error": repr(exc)}},
                [],
            )
            return
        self.send(
            {"id": msg_id, "kind": "anywidget-command-response", "response": response},
            result_buffers,
        )

    def _zarr_get(self, params: list, _buffers: list[bytes]) -> tuple:
        [store_name, key] = params
        store = SyncStoreWrapper(self._stores[store_name])
        try:
            buf = store.get(key.lstrip("/"), prototype=default_buffer_prototype())
            if buf is None:
                return {"success": False}, []
            return {"success": True}, [buf.to_bytes()]
        except Exception:  # noqa: BLE001
            return {"success": False}, []

    def _zarr_get_range(self, params: list, _buffers: list[bytes]) -> tuple:
        [store_name, key, range_query] = params
        store = SyncStoreWrapper(self._stores[store_name])
        try:
            if "suffixLength" in range_query:
                byte_range = SuffixByteRequest(suffix=range_query["suffixLength"])
            elif "offset" in range_query and "length" in range_query:
                byte_range = RangeByteRequest(
                    start=range_query["offset"],
                    end=range_query["offset"] + range_query["length"],
                )
            else:
                return {"success": False}, []
            buf = store.get(
                key.lstrip("/"),
                byte_range=byte_range,
                prototype=default_buffer_prototype(),
            )
            if buf is None:
                return {"success": False}, []
            return {"success": True}, [buf.to_bytes()]
        except Exception:  # noqa: BLE001
            return {"success": False}, []

    def _zarr_get_multi(self, params_arr: list, buffers: list[bytes]) -> tuple:
        result_dicts = []
        result_buffers = []
        for params in params_arr:
            if len(params) == 2:
                result_dict, result_buffer_arr = self._zarr_get(params, buffers)
            elif len(params) == 3:
                result_dict, result_buffer_arr = self._zarr_get_range(params, buffers)
            else:
                result_dict, result_buffer_arr = {"success": False}, []
            result_dicts.append(result_dict)
            result_buffers.append(
                result_buffer_arr[0] if result_dict["success"] and result_buffer_arr else b""
            )
        return result_dicts, result_buffers
