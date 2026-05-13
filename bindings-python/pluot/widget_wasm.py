# TODO: implement an anywidget that performs rendering via Pluot's WASM render function.
# I.e., perform frontend/client-side rendering.
# It should leverage the camera implementation from @pluot/core for interactivity.
# Zarr data should be transmitted via buffers sent via custom widget message type.

"""
# Zarr data should be requested via a custom widget message type:
# Partial Python snippet:
cmds = {}
def handle_anywidget_command(
    self: WidgetBase,
    msg: str | list | dict,
    buffers: list[bytes],
) -> None:
    if not isinstance(msg, dict) or msg.get("kind") != "anywidget-command":
        return
    cmd = cmds[msg["name"]]
    response, buffers = cmd(widget, msg["msg"], buffers)
    self.send(
        {
            "id": msg["id"],
            "kind": "anywidget-command-response",
            "response": response,
        },
        buffers,
    )

widget.on_msg(handle_anywidget_command)
"""


"""
# Python snippet:
@anywidget.experimental.command
def _zarr_get(self, params, buffers):
    [store_url, key] = params
    store = self._stores[store_url]
    try:
        buffers = [store[key.lstrip("/")]]
    except KeyError:
        buffers = []
    return {"success": len(buffers) == 1}, buffers

@anywidget.experimental.command
def _zarr_get_range(self, params, buffers):
    [store_url, key, range_query] = params
    store = self._stores[store_url]
    try:
        full_value = store[key.lstrip("/")]
        # Reference: https://github.com/manzt/zarrita.js/blob/f63a2521e2b46b22aa26af4146822e4d827dff83/packages/%40zarrita-storage/src/types.ts#L3
        if "suffixLength" in range_query:
            suffix_length = range_query["suffixLength"]
            buffers = [full_value[-suffix_length:]]
        elif "offset" in range_query and "length" in range_query:
            offset = range_query["offset"]
            length = range_query["length"]
            buffers = [full_value[offset:offset + length]]
    except KeyError:
        buffers = []
    return {"success": len(buffers) == 1}, buffers

@anywidget.experimental.command
def _zarr_get_multi(self, params_arr, buffers):
    # This variant of _zarr_get and _zarr_get_range supports batching.
    result_dicts = []
    result_buffers = []
    for params in params_arr:
        if len(params) == 2:
            result_dict, result_buffer_arr = self._zarr_get(params, buffers)
        elif len(params) == 3:
            result_dict, result_buffer_arr = self._zarr_get_range(params, buffers)

        if result_dict["success"] and len(result_buffer_arr) == 1:
            result_dicts.append(result_dict)
            result_buffers.append(result_buffer_arr[0])
        else:
            result_dicts.append({"success": False})
            result_buffers.append(b'')
    return result_dicts, result_buffers
"""

"""
// Partial JS snippet:
let pending = [];
let batchId = 0;

async function processBatch(prevPendingArr) {
    const [dataArr, buffersArr] = await view.experimental.invoke("_zarr_get_multi", prevPendingArr.map(d => d.params), {
        signal: AbortSignal.timeout(invokeTimeout),
    });
    prevPendingArr.forEach((prevPendingItem, i) => {
        const data = dataArr[i];
        const bufferData = buffersArr[i];
        const { params, resolve, reject } = prevPendingItem;
        const [storeUrl, key] = params;

        if (!data.success) {
            resolve(undefined);
            return;
        }

        if (ArrayBuffer.isView(bufferData)) {
            resolve(new Uint8Array(bufferData.buffer, bufferData.byteOffset, bufferData.byteLength));
            return;
        }
        resolve(new Uint8Array(bufferData.buffer));
        return;
    });
}

function run() {
    processBatch(pending);
    pending = [];
    batchId = 0;
}

function enqueue(params) {
    batchId = batchId || requestAnimationFrame(() => run());
    let { promise, resolve, reject } = Promise.withResolvers();
    pending.push({ params, resolve, reject });
    return promise;
}


// Partial JS snippet for custom Zarrita AsyncReadable that invokes the Python command functions:
{
    async get(key) {
        if (invokeBatched) {
            return enqueue([storeUrl, key]);
        } else {
            // Do not submit zarr gets in batches. Instead, submit individually.
            const [data, buffers] = await view.experimental.invoke("_zarr_get", [storeUrl, key], {
                signal: AbortSignal.timeout(invokeTimeout),
            });
            if (!data.success) return undefined;

            if (ArrayBuffer.isView(buffers[0])) {
                return new Uint8Array(buffers[0].buffer, buffers[0].byteOffset, buffers[0].byteLength);
            }
            return new Uint8Array(buffers[0].buffer);
        }
    },
    async getRange(key, rangeQuery) {
        if (invokeBatched) {
            return enqueue([storeUrl, key, rangeQuery]);
        } else {
            // Do not submit zarr gets in batches. Instead, submit individually.
            const [data, buffers] = await view.experimental.invoke("_zarr_get_range", [storeUrl, key, rangeQuery], {
                signal: AbortSignal.timeout(invokeTimeout),
            });
            if (!data.success) return undefined;

            if (ArrayBuffer.isView(buffers[0])) {
                return new Uint8Array(buffers[0].buffer, buffers[0].byteOffset, buffers[0].byteLength);
            }
            return new Uint8Array(buffers[0].buffer);
        }
    },
}

function invokePluginCommand(commandName, commandParams, commandBuffers) {
    return view.experimental.invoke("_plugin_command", [commandName, commandParams], {
        signal: AbortSignal.timeout(invokeTimeout),
        ...(commandBuffers ? { buffers: commandBuffers } : {}),
    });
}
"""
