"""AnyWidget that renders Pluot plots client-side via the WASM bindings.

The browser loads the Pluot WASM module and calls ``render_wasm`` directly.
Zarr data is served by the Python kernel via a custom message protocol: the JS
side sends ``anywidget-command`` messages, the Python handler fetches the
requested bytes from the registered zarr stores, and replies with
``anywidget-command-response`` messages carrying the raw bytes as binary buffers.
"""

from __future__ import annotations

import asyncio
import functools
import itertools
import logging
import os
import pathlib
import sys
import time
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

_LOGGER = logging.getLogger("pluot_widget")
# sys.__stderr__ is the terminal under CPython and the browser console under Pyodide,
# bypassing marimo's per-cell output capture.
_log_handler = logging.StreamHandler(sys.__stderr__)
_log_handler.setFormatter(logging.Formatter("[pluot_widget py %(relativeCreated)d ms] %(message)s"))
_LOGGER.addHandler(_log_handler)
_LOGGER.setLevel(logging.DEBUG)
_LOGGER.propagate = False

_instance_counter = itertools.count(1)

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

    def __init__(self, stores: dict | None = None, store: Store | None = None, **kwargs: Any) -> None:
        self._instance_number = next(_instance_counter)
        _LOGGER.debug(
            "__init__ start: instance=%d platform=%s store=%r kwargs=%r",
            self._instance_number, sys.platform, store, kwargs,
        )
        self._stores: dict = dict(stores or {})
        # The event loop only holds weak references to tasks.
        self._pending_tasks: set[asyncio.Task] = set()
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

        self.on_msg(self._handle_msg)
        self.observe(self._log_trait_change)
        _LOGGER.debug(
            "__init__ done: instance=%d model_id=%s stores_metadata=%r",
            self._instance_number, self.model_id, self.stores_metadata,
        )

    def _log_trait_change(self, change: dict) -> None:
        _LOGGER.debug(
            "trait change: instance=%d name=%s old=%r new=%r",
            self._instance_number, change["name"], change["old"], change["new"],
        )

    def close(self) -> None:
        _LOGGER.debug("close: instance=%d model_id=%s", self._instance_number, self.model_id)
        super().close()

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
                _LOGGER.debug(
                    "_handle_msg (1 arg) non-command: instance=%d data=%r",
                    self._instance_number, msg.get("content", {}).get("data"),
                )
                super()._handle_msg(*args)
                return
            self._dispatch_command(content, buffers)
            return
        elif len(args) == 3:
            # widget, content, buffers passed as positional args.
            # This happens in marimo?
            [_widget, content, buffers] = args
            if not isinstance(content, dict) or content.get("kind") != "anywidget-command":
                _LOGGER.debug(
                    "_handle_msg (3 args) non-command: instance=%d content=%r",
                    self._instance_number, content,
                )
                super()._handle_msg(*args)
                return
            self._dispatch_command(content, buffers)
        else:
            _LOGGER.debug("_handle_msg unexpected arg count: instance=%d args=%r", self._instance_number, args)

    def _dispatch_command(self, msg: dict, _buffers: list[bytes]) -> None:
        name = msg.get("name")
        msg_id = msg.get("id")
        _LOGGER.debug(
            "command received: instance=%d id=%s name=%s params=%r",
            self._instance_number, msg_id, name, msg.get("msg"),
        )
        if name not in ("_zarr_get", "_zarr_get_range", "_zarr_get_multi"):
            _LOGGER.debug("command ignored: id=%s name=%s", msg_id, name)
            return
        command = self._run_command(name, msg.get("msg"))
        started_at = time.perf_counter()
        if sys.platform == "emscripten":
            # Pyodide cannot start the IO thread that zarr's sync() relies on,
            # but its event loop is already running, so schedule the reads on it instead.
            task = asyncio.get_event_loop().create_task(command)
            self._pending_tasks.add(task)
            _LOGGER.debug("command scheduled as task: id=%s pending=%d", msg_id, len(self._pending_tasks))
            task.add_done_callback(functools.partial(self._on_command_done, msg_id, started_at))
        else:
            self._send_response(msg_id, started_at, *zarr_sync(command))

    def _on_command_done(self, msg_id: str, started_at: float, task: asyncio.Task) -> None:
        self._pending_tasks.discard(task)
        try:
            result = task.result()
        except BaseException as exc:  # noqa: BLE001
            _LOGGER.debug("command task failed: id=%s exc=%r", msg_id, exc)
            raise
        self._send_response(msg_id, started_at, *result)

    def _send_response(self, msg_id: str, started_at: float, response: Any, result_buffers: list[bytes]) -> None:
        _LOGGER.debug(
            "command response: instance=%d id=%s elapsed_ms=%.1f response=%r buffer_lengths=%r",
            self._instance_number, msg_id, (time.perf_counter() - started_at) * 1000,
            response, [len(b) for b in result_buffers],
        )
        self.send(
            {"id": msg_id, "kind": "anywidget-command-response", "response": response},
            result_buffers,
        )

    async def _run_command(self, name: str, params: list) -> tuple:
        try:
            if name == "_zarr_get":
                return await self._zarr_get(params)
            if name == "_zarr_get_range":
                return await self._zarr_get_range(params)
            return await self._zarr_get_multi(params)
        except Exception as exc:  # noqa: BLE001
            _LOGGER.debug("command raised: name=%s params=%r", name, params, exc_info=True)
            return {"error": repr(exc)}, []

    async def _zarr_get(self, params: list) -> tuple:
        [store_name, key] = params
        buf = await self._stores[store_name].get(key.lstrip("/"), prototype=default_buffer_prototype())
        if buf is None:
            return {"success": False}, []
        return {"success": True}, [buf.to_bytes()]

    async def _zarr_get_range(self, params: list) -> tuple:
        [store_name, key, range_query] = params
        if "suffixLength" in range_query:
            byte_range = SuffixByteRequest(suffix=range_query["suffixLength"])
        elif "offset" in range_query and "length" in range_query:
            byte_range = RangeByteRequest(
                start=range_query["offset"],
                end=range_query["offset"] + range_query["length"],
            )
        else:
            return {"success": False}, []
        buf = await self._stores[store_name].get(
            key.lstrip("/"),
            byte_range=byte_range,
            prototype=default_buffer_prototype(),
        )
        if buf is None:
            return {"success": False}, []
        return {"success": True}, [buf.to_bytes()]

    async def _zarr_get_any(self, params: list) -> tuple:
        if len(params) == 2:
            return await self._zarr_get(params)
        if len(params) == 3:
            return await self._zarr_get_range(params)
        return {"success": False}, []

    async def _zarr_get_multi(self, params_arr: list) -> tuple:
        results = await asyncio.gather(*(self._zarr_get_any(params) for params in params_arr))
        result_dicts = [result_dict for result_dict, _ in results]
        result_buffers = [
            result_buffer_arr[0] if result_dict["success"] else b""
            for result_dict, result_buffer_arr in results
        ]
        return result_dicts, result_buffers
