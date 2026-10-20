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
    """AnyWidget that renders a Pluot plot client-side using the WASM bindings.

    Every traitlet below is synced and described by its ``help``, so it can be
    passed to the constructor as a keyword argument, and changing it from Python
    updates the plot. Traitlets that the plot updates in response to user
    interactions (``camera_view``, ``extent``, ``click_result``, ``brush``, and
    ``brushing_result``) can be observed via ``widget.observe``.

    Examples
    --------
    Create a widget that reads point coordinates from a zarr store. Layers refer
    to arrays by their keys within the store.

    >>> from zarr.storage import LocalStore
    >>> def make_plot_params(point_radius):
    ...     return dict(layers=[dict(
    ...         layer_type="ZarrPointLayer",
    ...         layer_params=dict(
    ...             layer_id="points",
    ...             x_key="/x",
    ...             y_key="/y",
    ...             point_radius=point_radius,
    ...         ),
    ...     )])
    >>> widget = PluotWasmWidget(
    ...     store=LocalStore("points.zarr"),
    ...     store_name="points",
    ...     width=600,
    ...     height=400,
    ...     plot_params=make_plot_params(3.0),
    ... )
    >>> widget  # Display it as the last expression of a notebook cell.

    Update the plot by assigning traitlets, rather than by creating a new widget.
    In marimo, this means that the cell which creates the widget must not
    depend on UI element values, since marimo would re-run it and re-create
    the widget. Instead, assign the values in a separate cell:

    >>> widget.plot_params = make_plot_params(point_radius_slider.value)
    >>> widget.width = 800

    React to user interactions by observing the traitlets that the plot syncs
    back, such as the camera matrix after the user zooms or pans:

    >>> widget.observe(lambda change: print(change["new"]), names=["camera_view"])

    In marimo, route the observed value through ``mo.state``, so that cells
    which read it re-run when it changes:

    >>> get_camera, set_camera = mo.state(widget.camera_view)
    >>> widget.observe(lambda _: set_camera(widget.camera_view), names=["camera_view"])
    """

    _esm = _ESM

    # Layout and camera state.
    width = traitlets.Int(800, help="Width of the plot, in pixels.").tag(sync=True)
    height = traitlets.Int(800, help="Height of the plot, in pixels.").tag(sync=True)
    camera_view = traitlets.List(
        trait=traitlets.Float(),
        default_value=None,
        allow_none=True,
        help=(
            "The 4x4 camera matrix, flattened in column-major order. None fits the camera to x_lim/y_lim, "
            "after which the fitted matrix is synced back here. Setting it to None again refits."
        ),
    ).tag(sync=True)
    x_lim = traitlets.List(
        trait=traitlets.Float(), minlen=2, maxlen=2, default_value=None, allow_none=True,
        help=(
            "X data limits as [min, max]. None fits this axis to the data extent. "
            "Changing x_lim or y_lim refits the camera, overriding camera_view."
        ),
    ).tag(sync=True)
    y_lim = traitlets.List(
        trait=traitlets.Float(), minlen=2, maxlen=2, default_value=None, allow_none=True,
        help=(
            "Y data limits as [min, max]. None fits this axis to the data extent. "
            "Changing x_lim or y_lim refits the camera, overriding camera_view."
        ),
    ).tag(sync=True)
    extent = traitlets.Dict(
        default_value=None, allow_none=True,
        help="The ExtentResult from the most recent fit that needed the data extent.",
    ).tag(sync=True)
    margin_top = traitlets.Float(0.0, help="Top margin around the plot area, in pixels, e.g., to make room for an axis.").tag(sync=True)
    margin_right = traitlets.Float(0.0, help="Right margin around the plot area, in pixels, e.g., to make room for an axis.").tag(sync=True)
    margin_bottom = traitlets.Float(0.0, help="Bottom margin around the plot area, in pixels, e.g., to make room for an axis.").tag(sync=True)
    margin_left = traitlets.Float(0.0, help="Left margin around the plot area, in pixels, e.g., to make room for an axis.").tag(sync=True)
    aspect_ratio_mode = traitlets.Unicode(
        "Contain",
        help=(
            'How the aspect ratio of the plot area is reconciled with that of the data: "Contain" preserves it by showing '
            'more data along the longer dimension, "Cover" preserves it by clipping data along the shorter '
            'dimension, and "Ignore" stretches the data to fill the plot area.'
        ),
    ).tag(sync=True)
    aspect_ratio_alignment_mode = traitlets.Unicode(
        "Center",
        help='Where the data is aligned within the plot area under "Contain" or "Cover": "Center", "Start", or "End".',
    ).tag(sync=True)
    view_mode = traitlets.Unicode("2d", help='"2d" or "3d".').tag(sync=True)
    camera_filter = traitlets.Enum(
        [
            "fixX",
            "fixY",
            "fixXAxisAtYZero",
            "fixYAxisAtXZero",
            "fixXAndFixXAxisAtYZero",
            "fixYAndFixYAxisAtXZero",
        ],
        default_value=None,
        allow_none=True,
        help=(
            'Constrains camera interactions. "fixX"/"fixY" prevent zooming and panning along X/Y. '
            '"fixXAxisAtYZero"/"fixYAxisAtXZero" keep data zero pinned to the edge of the plot area, '
            "so that an axis stays at zero. The remaining options combine the two."
        ),
    ).tag(sync=True)
    debug_margins = traitlets.Bool(False, help="Outlines the margin box and the plot area, to help debug layout.").tag(sync=True)
    background_color = traitlets.Unicode(
        None, allow_none=True, help="Any CSS color. None leaves the background transparent.",
    ).tag(sync=True)

    # Picking.
    enable_click = traitlets.Bool(False, help="Whether clicking runs a picking query and updates click_result.").tag(sync=True)
    enable_tooltip = traitlets.Bool(False, help="Whether hovering runs a picking query and shows a tooltip.").tag(sync=True)
    click_result = traitlets.Dict(
        default_value=None, allow_none=True,
        help="The PickingResult of the most recent click, when enable_click is set.",
    ).tag(sync=True)

    # Brushing.
    enable_brush_create = traitlets.Bool(
        False, help="Whether the user can draw a brush by long-clicking and then dragging.",
    ).tag(sync=True)
    enable_brush_edit = traitlets.Bool(
        False, help="Whether the user can modify the brush by dragging its vertices or sides.",
    ).tag(sync=True)
    enable_brush_clear = traitlets.Bool(False, help="Whether hovering the brush shows a button that clears it.").tag(sync=True)
    brush_mode = traitlets.Enum(
        ["Rect", "Polygon", "RangeX", "RangeY"], default_value="Rect",
        help="The shape of brush that the user draws.",
    ).tag(sync=True)
    brush_units_mode_x = traitlets.Enum(
        ["Pixels", "Data", "Normalized"], default_value="Data",
        help='Units of the brush X coordinates. With "Data", the brush is tied to the data and moves on screen as the camera changes.',
    ).tag(sync=True)
    brush_units_mode_y = traitlets.Enum(
        ["Pixels", "Data", "Normalized"], default_value="Data",
        help='Units of the brush Y coordinates. With "Data", the brush is tied to the data and moves on screen as the camera changes.',
    ).tag(sync=True)
    brush_margin_top = traitlets.Float(
        None, allow_none=True,
        help='Top margin of the brushable region, in pixels. None falls back to margin_top. Ignored when brush_units_mode_y is "Data".',
    ).tag(sync=True)
    brush_margin_right = traitlets.Float(
        None, allow_none=True,
        help='Right margin of the brushable region, in pixels. None falls back to margin_right. Ignored when brush_units_mode_x is "Data".',
    ).tag(sync=True)
    brush_margin_bottom = traitlets.Float(
        None, allow_none=True,
        help='Bottom margin of the brushable region, in pixels. None falls back to margin_bottom. Ignored when brush_units_mode_y is "Data".',
    ).tag(sync=True)
    brush_margin_left = traitlets.Float(
        None, allow_none=True,
        help='Left margin of the brushable region, in pixels. None falls back to margin_left. Ignored when brush_units_mode_x is "Data".',
    ).tag(sync=True)
    brush_color = traitlets.Unicode(
        "#3b6ea5", help="CSS color of the brush overlay. The fill uses it at reduced opacity.",
    ).tag(sync=True)
    brush_delay = traitlets.Int(
        1500, help="How long, in milliseconds, the user must long-click to start drawing a brush.",
    ).tag(sync=True)
    maybe_brush_delay = traitlets.Int(
        250, help="How long, in milliseconds, the user must click-and-hold before a progress indicator for brush_delay appears.",
    ).tag(sync=True)
    brush = traitlets.Dict(
        default_value=None, allow_none=True,
        help=(
            "The BrushState, updated when the user finishes drawing or editing the brush, or clears it. "
            "Can also be set from Python to draw a brush. None means nothing is brushed."
        ),
    ).tag(sync=True)
    brushing_result = traitlets.Dict(
        default_value=None, allow_none=True,
        help="The BrushingResult for brush. Only recomputed for user interactions, not when brush is set from Python.",
    ).tag(sync=True)

    # Plot config forwarded to render_wasm.
    plot_id = traitlets.Unicode(
        "plot", help="Plot ID, unique per page, used to key caches of intermediate values.",
    ).tag(sync=True)
    plot_type = traitlets.Unicode("LayeredPlot", help="The type of plot that plot_params describes.").tag(sync=True)
    store_name = traitlets.Unicode("", help="The name that the constructor's store is registered under.").tag(sync=True)
    stores_metadata = traitlets.Dict(
        default_value={},
        help="Portable ZarrStoreInfo metadata for each registered store, keyed by store name. Derived from the constructor's stores/store.",
    ).tag(sync=True)
    plot_params = traitlets.Dict(
        default_value={}, help='The plot parameters, e.g., {"layers": [...]} for a "LayeredPlot".',
    ).tag(sync=True)
    format = traitlets.Unicode(
        "Raster",
        help='"Raster" renders to a canvas via WebGPU. "Vector" renders to SVG, which does not require a GPU.',
    ).tag(sync=True)
    batch_zarr_gets = traitlets.Bool(
        False, help="Whether zarr reads requested in the same animation frame are sent to the kernel as a single message.",
    ).tag(sync=True)
    min_timeout = traitlets.Int(
        300, help="Lower bound, in milliseconds, of the exponential backoff between retries of renders that bailed early.",
    ).tag(sync=True)
    max_timeout = traitlets.Int(
        5000, help="Upper bound, in milliseconds, of the exponential backoff between retries of renders that bailed early.",
    ).tag(sync=True)
    max_bailed_early_retries = traitlets.Int(
        300, help="Number of times a render that bailed early may retry before giving up.",
    ).tag(sync=True)
    allow_simultaneous_renders = traitlets.Bool(
        True, help="Whether a new render may start while a previous one is still in flight.",
    ).tag(sync=True)
    camera_sync_interval = traitlets.Int(
        250, help="Minimum time, in milliseconds, between syncs of camera_view back to the kernel during camera interactions.",
    ).tag(sync=True)

    # JS dependency loading. Only read when the widget is rendered.
    js_dev_mode = traitlets.Bool(
        False,
        help=(
            "Whether to load development builds of React, for React dev warnings. A page shares "
            "one React instance, so the first widget rendered on the page decides this."
        ),
    ).tag(sync=True)
    js_package_version = traitlets.Unicode(
        "", help="Version of the @pluot/react NPM package to load. Empty uses the version the widget was built against.",
    ).tag(sync=True)
    custom_js_url = traitlets.Unicode(
        "",
        help=(
            "URL of an ES module build of @pluot/react (e.g., a local `pnpm run bundle` output) "
            "to load instead of the published version. Takes precedence over js_package_version."
        ),
    ).tag(sync=True)

    def __init__(self, stores: dict | None = None, store: Store | None = None, **kwargs: Any) -> None:
        """Create the widget and register the zarr stores that its layers read from.

        Parameters
        ----------
        stores : dict of str to zarr.abc.store.Store, optional
            Zarr stores keyed by store name.
        store : zarr.abc.store.Store, optional
            A single zarr store, registered in addition to ``stores``. It is
            registered under ``store_name`` if given, and otherwise under a
            generated name, which is then available as ``widget.store_name``.
        **kwargs
            Initial values for any of the traitlets defined on the class,
            e.g., ``width``, ``height``, or ``plot_params``. Use ``help(PluotWasmWidget)``
            to list the available kwargs and their descriptions.
        """
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
        """Register another zarr store for the kernel to serve reads from.

        Parameters
        ----------
        name : str
            The store name, which layer parameters can use to refer to it.
        store : zarr.abc.store.Store
            The zarr store.

        Notes
        -----
        This does not update ``stores_metadata``, so the browser only reads from
        this store if it already knows the name, e.g., from the constructor.
        """
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
