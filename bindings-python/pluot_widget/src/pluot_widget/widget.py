"""AnyWidget that renders Pluot plots client-side via the WASM bindings.

The browser loads the Pluot WASM module and calls ``render_wasm`` directly.
Zarr data is served by the Python kernel via a custom message protocol: the JS
side sends ``anywidget-command`` messages, the Python handler fetches the
requested bytes from the registered zarr stores, and replies with
``anywidget-command-response`` messages carrying the raw bytes as binary buffers.
"""

from __future__ import annotations

import asyncio
import os
import pathlib
import sys
import uuid
from typing import Any

import anywidget
from pluot_core.zarr import store_instance_to_metadata
import traitlets
from zarr.abc.store import RangeByteRequest, Store, SuffixByteRequest
from zarr.core.buffer.core import default_buffer_prototype
from zarr.core.sync import sync as zarr_sync


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

if os.getenv("ANYWIDGET_HMR"):
    _ESM = "http://localhost:5183/js/widget.tsx?anywidget"
else:
    _ESM = pathlib.Path(__file__).parent / "static" / "widget.js"


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
    # The serializable stores metadata dict.
    stores_metadata = traitlets.Dict(default_value={}).tag(sync=True)
    plot_params = traitlets.Dict(default_value={}).tag(sync=True)
    format = traitlets.Unicode("Raster").tag(sync=True)
    batch_zarr_gets = traitlets.Bool(False).tag(sync=True)
    # Milliseconds.
    min_timeout = traitlets.Int(300).tag(sync=True)
    # Milliseconds.
    camera_sync_interval = traitlets.Int(250).tag(sync=True)

    def __init__(self, stores: dict | None = None, store: Store | None = None, **kwargs: Any) -> None:
        self._stores: dict = dict(stores or {})
        if store is not None:
            store_name = kwargs.get("store_name") if "store_name" in kwargs else str(id(store))
            self._stores[store_name] = store
            self.store_name = store_name

        stores_metadata = {}
        for store_key, store_instance in self._stores.items():
            stores_metadata[store_key] = store_instance_to_metadata(store_instance)

        # We need to call the anywidget/ipywidgets constructor _after_
        # defining stores_metadata, so that we can pass it as the initial traitlet value.
        # Reference: https://github.com/vitessce/vitessce-python/blob/34376752fd056d3da4f7fc0bc63c9172179f75f3/src/vitessce/widget.py#L863
        super(PluotWasmWidget, self).__init__(
            **kwargs, stores_metadata=stores_metadata,
        )

        self._pending_commands: set[asyncio.Task] = set()
        self.on_msg(self._handle_msg)

    def add_store(self, name: str, store: Any) -> None:
        self._stores[name] = store

    def _handle_msg(self, *args) -> None:
        if len(args) == 1:
            # msg passed as positional argument.
            # This happens in jupyter lab?
            msg = args[0]
            content = msg.get("content", {}).get("data", {}).get("content", {})
            buffers = msg.get("buffers", [])
            if not isinstance(content, dict) or content.get("kind") != "anywidget-command":
                super()._handle_msg(*args)
                return
            self._dispatch_command(content, buffers)
            return
        elif len(args) == 3:
            # widget, content, buffers passed as positional args.
            # This happens in marimo?
            [_widget, content, buffers] = args
            if not isinstance(content, dict) or content.get("kind") != "anywidget-command":
                super()._handle_msg(*args)
                return
            self._dispatch_command(content, buffers)

    def _dispatch_command(self, msg: dict, buffers: list[bytes]) -> None:
        msg_id = msg.get("id")
        command = self._run_command(msg.get("name"), msg.get("msg"), buffers)
        if sys.platform == "emscripten":
            # zarr's sync() runs coroutines on a dedicated IO thread, and Pyodide cannot
            # start threads. Schedule on the already-running WebLoop instead.
            task = asyncio.ensure_future(command)
            self._pending_commands.add(task)
            task.add_done_callback(self._pending_commands.discard)
            task.add_done_callback(lambda t: self._send_command_response(msg_id, t.result()))
        else:
            self._send_command_response(msg_id, zarr_sync(command))

    def _send_command_response(self, msg_id: str, result: tuple | None) -> None:
        if result is None:
            return
        response, result_buffers = result
        self.send(
            {"id": msg_id, "kind": "anywidget-command-response", "response": response},
            result_buffers,
        )

    async def _run_command(self, name: str, params: Any, buffers: list[bytes]) -> tuple | None:
        try:
            if name == "_zarr_get":
                return await self._zarr_get(params, buffers)
            if name == "_zarr_get_range":
                return await self._zarr_get_range(params, buffers)
            if name == "_zarr_get_multi":
                return await self._zarr_get_multi(params, buffers)
            return None
        except Exception as exc:  # noqa: BLE001
            return {"error": repr(exc)}, []

    async def _zarr_get(self, params: list, _buffers: list[bytes]) -> tuple:
        [store_name, key] = params
        store = self._stores[store_name]
        try:
            buf = await store.get(key.lstrip("/"), prototype=default_buffer_prototype())
            if buf is None:
                return {"success": False}, []
            return {"success": True}, [buf.to_bytes()]
        except Exception:  # noqa: BLE001
            return {"success": False}, []

    async def _zarr_get_range(self, params: list, _buffers: list[bytes]) -> tuple:
        [store_name, key, range_query] = params
        store = self._stores[store_name]
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
            buf = await store.get(
                key.lstrip("/"),
                byte_range=byte_range,
                prototype=default_buffer_prototype(),
            )
            if buf is None:
                return {"success": False}, []
            return {"success": True}, [buf.to_bytes()]
        except Exception:  # noqa: BLE001
            return {"success": False}, []

    async def _zarr_get_one(self, params: list, buffers: list[bytes]) -> tuple:
        if len(params) == 2:
            return await self._zarr_get(params, buffers)
        if len(params) == 3:
            return await self._zarr_get_range(params, buffers)
        return {"success": False}, []

    async def _zarr_get_multi(self, params_arr: list, buffers: list[bytes]) -> tuple:
        results = await asyncio.gather(*(self._zarr_get_one(params, buffers) for params in params_arr))
        result_dicts = [result_dict for result_dict, _ in results]
        result_buffers = [
            result_buffer_arr[0] if result_dict["success"] and result_buffer_arr else b""
            for result_dict, result_buffer_arr in results
        ]
        return result_dicts, result_buffers
