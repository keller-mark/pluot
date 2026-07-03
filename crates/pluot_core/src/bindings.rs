pub use crate::params::RenderParams;
pub use crate::render::render;
pub use crate::picking::{pick, PickingResult};
pub use crate::viewport::ScreenCoord;
pub use crate::zarr_types::ZarrPeekResult;


// == WASM Bindings ===
#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use super::{render, pick, RenderParams, ScreenCoord, ZarrPeekResult};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    #[derive(Copy, Clone, Debug)]
    pub enum JsZarrPeekResult {
        // Reference: https://wasm-bindgen.github.io/wasm-bindgen/reference/types/imported-js-types.html
        Pending = "pending",
        Fulfilled = "fulfilled",
        Rejected = "rejected",
    }

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console)]
        pub fn log(s: &str);
    }

    // This is a hack that allows avoiding putting these functions on `window` or `globalThis`.
    // It is a workaround for https://github.com/wasm-bindgen/wasm-bindgen/issues/3041
    #[wasm_bindgen(inline_js = "
let __zarr_impl = {};

export function __js_set_zarr_imports(impl) {
    __zarr_impl = impl;
}

export function zarr_has(store_name, key) {
    return __zarr_impl.zarr_has(store_name, key);
}

export function zarr_has_status(store_name, key) {
    return __zarr_impl.zarr_has_status(store_name, key);
}

export function zarr_get(store_name, key) {
    return __zarr_impl.zarr_get(store_name, key);
}

export function zarr_get_status(store_name, key) {
    return __zarr_impl.zarr_get_status(store_name, key);
}

export function zarr_get_range_from_offset(store_name, key, offset, length) {
    return __zarr_impl.zarr_get_range_from_offset(store_name, key, offset, length);
}

export function zarr_get_range_from_offset_status(store_name, key, offset, length) {
    return __zarr_impl.zarr_get_range_from_offset_status(store_name, key, offset, length);
}

export function zarr_get_range_from_end(store_name, key, suffix_length) {
    return __zarr_impl.zarr_get_range_from_end(store_name, key, suffix_length);
}

export function zarr_get_range_from_end_status(store_name, key, suffix_length) {
    return __zarr_impl.zarr_get_range_from_end_status(store_name, key, suffix_length);
}
")]
    extern "C" {
        #[wasm_bindgen(js_name = __js_set_zarr_imports)]
        fn set_zarr_imports_internal(impl_: JsValue);

        // We need to define a `has` function, since the zarr_get_js function
        // may return undefined, and wasm_bindgen does not allow
        // Option<Uint8Array> as a return type annotation.
        #[wasm_bindgen(js_name = zarr_has)]
        async fn zarr_has_js(store_name: &str, key: &str) -> js_sys::Boolean;

        #[wasm_bindgen(js_name = zarr_has_status)]
        fn zarr_has_status_js(store_name: &str, key: &str) -> JsZarrPeekResult;

        #[wasm_bindgen(js_name = zarr_get)]
        async fn zarr_get_js(store_name: &str, key: &str) -> js_sys::Uint8Array;

        #[wasm_bindgen(js_name = zarr_get_status)]
        fn zarr_get_status_js(store_name: &str, key: &str) -> JsZarrPeekResult;

        #[wasm_bindgen(js_name = zarr_get_range_from_offset)]
        async fn zarr_get_range_from_offset_js(
            store_name: &str,
            key: &str,
            offset: u32,
            length: u32,
        ) -> js_sys::Uint8Array;

        #[wasm_bindgen(js_name = zarr_get_range_from_offset_status)]
        fn zarr_get_range_from_offset_status_js(
            store_name: &str,
            key: &str,
            offset: u32,
            length: u32,
        ) -> JsZarrPeekResult;

        #[wasm_bindgen(js_name = zarr_get_range_from_end)]
        async fn zarr_get_range_from_end_js(
            store_name: &str,
            key: &str,
            suffix_length: u32,
        ) -> js_sys::Uint8Array;

        #[wasm_bindgen(js_name = zarr_get_range_from_end_status)]
        fn zarr_get_range_from_end_status_js(
            store_name: &str,
            key: &str,
            suffix_length: u32,
        ) -> JsZarrPeekResult;
    }

    /// Set the zarr store implementations. Must be called after `wasm.default()`.
    /// The `imports` object must contain the following functions:
    /// `zarr_get`, `zarr_get_status`, `zarr_has`, `zarr_has_status`,
    /// `zarr_get_range_from_offset`, `zarr_get_range_from_offset_status`,
    /// `zarr_get_range_from_end`, `zarr_get_range_from_end_status`.
    #[wasm_bindgen]
    pub fn set_zarr_imports(imports: JsValue) {
        set_zarr_imports_internal(imports);
    }

    fn convert_peek_result(status: JsZarrPeekResult) -> ZarrPeekResult {
        match status {
            JsZarrPeekResult::Pending => ZarrPeekResult::Pending,
            JsZarrPeekResult::Fulfilled => ZarrPeekResult::Fulfilled,
            JsZarrPeekResult::Rejected => ZarrPeekResult::Rejected,
            _ => panic!("Invalid JsZarrPeekResult"),
        }
    }

    /// Push the current Promise status for a (store, cache key) pair.
    /// The JS side must push Promise state changes (via this function,
    /// `zarr_remove_promise_status`, and `zarr_clear_promise_statuses`) whenever
    /// rendering with `wait_for_store_pushes: true`.
    /// `key` must be the JS-side cache key (with leading slash and any byte-range suffix).
    #[wasm_bindgen]
    pub fn zarr_push_promise_status(store_name: &str, key: &str, status: JsZarrPeekResult) {
        crate::zarr::push_promise_status(store_name, key, convert_peek_result(status));
    }

    /// Forget the Promise status for a (store, cache key) pair (e.g., on LRU eviction).
    #[wasm_bindgen]
    pub fn zarr_remove_promise_status(store_name: &str, key: &str) {
        crate::zarr::remove_promise_status(store_name, key);
    }

    /// Forget all Promise statuses for a store (e.g., on cache clear or store replacement).
    #[wasm_bindgen]
    pub fn zarr_clear_promise_statuses(store_name: &str) {
        crate::zarr::clear_promise_statuses(store_name);
    }

    fn convert_to_bytes(u8arr: js_sys::Uint8Array) -> zarrs::storage::Bytes {
        // Copy data from Uint8Array into a Rust Vec<u8>
        let mut vec = vec![0u8; u8arr.length() as usize];

        // TODO: can this be done without copying?
        // The issue is that the original Uint8Array is created via JS fetch() within zarrita fetchStore.

        u8arr.copy_to(&mut vec);

        // Convert Vec<u8> into Bytes
        zarrs::storage::Bytes::from(vec)
    }

    pub async fn zarr_has(store_name: &str, key: &str) -> bool {
        let has = zarr_has_js(store_name, key).await;
        has.is_truthy()
    }

    pub fn zarr_has_status(store_name: &str, key: &str) -> ZarrPeekResult {
        let status_js = zarr_has_status_js(store_name, key);
        match status_js {
            JsZarrPeekResult::Pending => ZarrPeekResult::Pending,
            JsZarrPeekResult::Fulfilled => ZarrPeekResult::Fulfilled,
            JsZarrPeekResult::Rejected => ZarrPeekResult::Rejected,
            _ => panic!("Invalid JsZarrPeekResult"),
        }
    }

    pub async fn zarr_get(store_name: &str, key: &str) -> zarrs::storage::Bytes {
        let js_bytes = zarr_get_js(store_name, key).await;
        convert_to_bytes(js_bytes)
    }

    pub fn zarr_get_status(store_name: &str, key: &str) -> ZarrPeekResult {
        let status_js = zarr_get_status_js(store_name, key);
        match status_js {
            JsZarrPeekResult::Pending => ZarrPeekResult::Pending,
            JsZarrPeekResult::Fulfilled => ZarrPeekResult::Fulfilled,
            JsZarrPeekResult::Rejected => ZarrPeekResult::Rejected,
            _ => panic!("Invalid JsZarrPeekResult"),
        }
    }

    pub async fn zarr_get_range_from_offset(
        store_name: &str,
        key: &str,
        offset: u32,
        length: u32,
    ) -> zarrs::storage::Bytes {
        let js_bytes = zarr_get_range_from_offset_js(store_name, key, offset, length).await;
        convert_to_bytes(js_bytes)
    }

    pub fn zarr_get_range_from_offset_status(
        store_name: &str,
        key: &str,
        offset: u32,
        length: u32,
    ) -> ZarrPeekResult {
        let status_js = zarr_get_range_from_offset_status_js(store_name, key, offset, length);
        match status_js {
            JsZarrPeekResult::Pending => ZarrPeekResult::Pending,
            JsZarrPeekResult::Fulfilled => ZarrPeekResult::Fulfilled,
            JsZarrPeekResult::Rejected => ZarrPeekResult::Rejected,
            _ => panic!("Invalid JsZarrPeekResult"),
        }
    }

    pub async fn zarr_get_range_from_end(
        store_name: &str,
        key: &str,
        suffix_length: u32,
    ) -> zarrs::storage::Bytes {
        let js_bytes = zarr_get_range_from_end_js(store_name, key, suffix_length).await;
        convert_to_bytes(js_bytes)
    }

    pub fn zarr_get_range_from_end_status(
        store_name: &str,
        key: &str,
        suffix_length: u32,
    ) -> ZarrPeekResult {
        let status_js = zarr_get_range_from_end_status_js(store_name, key, suffix_length);
        match status_js {
            JsZarrPeekResult::Pending => ZarrPeekResult::Pending,
            JsZarrPeekResult::Fulfilled => ZarrPeekResult::Fulfilled,
            JsZarrPeekResult::Rejected => ZarrPeekResult::Rejected,
            _ => panic!("Invalid JsZarrPeekResult"),
        }
    }

    #[wasm_bindgen]
    pub fn set_panic_hook() {
        // When the `console_error_panic_hook` feature is enabled, we can call the
        // `set_panic_hook` function at least once during initialization, and then
        // we will get better error messages if our code ever panics.
        //
        // For more details see
        // https://github.com/rustwasm/console_error_panic_hook#readme
        console_error_panic_hook::set_once();
    }

    // This function should accept width and height as parameters,
    // and return a Uint8Array containing the rendered image data.
    #[wasm_bindgen]
    pub async fn render_wasm(params: JsValue) -> js_sys::Uint8Array {
        let params: RenderParams =
            serde_wasm_bindgen::from_value(params).expect("Invalid parameters");

        let pixels = render(params).await;

        // Return a Uint8Array of RGBA bytes
        js_sys::Uint8Array::from(pixels.as_slice())
    }

    #[wasm_bindgen]
    pub async fn pick_wasm(params: JsValue, screen_x: f32, screen_y: f32) -> JsValue {
        let params: RenderParams =
            serde_wasm_bindgen::from_value(params).expect("Invalid parameters");

        let screen_coord = ScreenCoord { x: screen_x, y: screen_y };
        let result = pick(params, screen_coord).await;

        serde_wasm_bindgen::to_value(&result).expect("Failed to serialize PickingResult")
    }
}

// === Python Bindings ===
#[cfg(all(not(target_arch = "wasm32"), feature = "python"))]
pub mod python {
    use log::info;
    use pyo3::prelude::*;
    use pyo3::types::{PyAny, PyBytes, PyDict, PyTuple};
    use pyo3::wrap_pyfunction;
    use pyo3::IntoPyObject;
    use pyo3_log::{Caching, Logger};
    use pythonize::depythonize;

    use super::{render, pick, RenderParams, ScreenCoord, ZarrPeekResult};

    #[pyfunction]
    pub fn log_info(s: &str) {
        info!("{}", s);
    }

    pub fn zarr_has_status(store_name: &str, key: &str) -> ZarrPeekResult {
        Python::attach(|py| {
            let zarr_module = PyModule::import(py, "pluot.zarr").unwrap();
            let result = zarr_module.call_method1("zarr_has_status", (store_name, key)).unwrap();
            let value: u8 = result.extract().unwrap();
            match value {
                0 => ZarrPeekResult::Pending,
                1 => ZarrPeekResult::Fulfilled,
                2 => ZarrPeekResult::Rejected,
                _ => panic!("Invalid ZarrPeekResult value from Python"),
            }
        })
    }

    pub fn zarr_get_status(store_name: &str, key: &str) -> ZarrPeekResult {
        Python::attach(|py| {
            let zarr_module = PyModule::import(py, "pluot.zarr").unwrap();
            let result = zarr_module.call_method1("zarr_get_status", (store_name, key)).unwrap();
            let value: u8 = result.extract().unwrap();
            match value {
                0 => ZarrPeekResult::Pending,
                1 => ZarrPeekResult::Fulfilled,
                2 => ZarrPeekResult::Rejected,
                _ => panic!("Invalid ZarrPeekResult value from Python"),
            }
        })
    }

    pub fn zarr_get_range_from_offset_status(
        store_name: &str,
        key: &str,
        offset: u32,
        length: u32,
    ) -> ZarrPeekResult {
        Python::attach(|py| {
            let zarr_module = PyModule::import(py, "pluot.zarr").unwrap();
            let result = zarr_module
                .call_method1("zarr_get_range_from_offset_status", (store_name, key, offset, length))
                .unwrap();
            let value: u8 = result.extract().unwrap();
            match value {
                0 => ZarrPeekResult::Pending,
                1 => ZarrPeekResult::Fulfilled,
                2 => ZarrPeekResult::Rejected,
                _ => panic!("Invalid ZarrPeekResult value from Python"),
            }
        })
    }

    pub fn zarr_get_range_from_end_status(
        store_name: &str,
        key: &str,
        suffix_length: u32,
    ) -> ZarrPeekResult {
        Python::attach(|py| {
            let zarr_module = PyModule::import(py, "pluot.zarr").unwrap();
            let result = zarr_module
                .call_method1("zarr_get_range_from_end_status", (store_name, key, suffix_length))
                .unwrap();
            let value: u8 = result.extract().unwrap();
            match value {
                0 => ZarrPeekResult::Pending,
                1 => ZarrPeekResult::Fulfilled,
                2 => ZarrPeekResult::Rejected,
                _ => panic!("Invalid ZarrPeekResult value from Python"),
            }
        })
    }

    pub async fn zarr_has(store_name: &str, key: &str) -> bool {
        // Acquire the Python GIL. This must be done for all Python interactions.
        let py_obj = Python::attach(|py| {
            let zarr_module = PyModule::import(py, "pluot.zarr").unwrap();

            // Call the async function, which returns a coroutine
            let coroutine = zarr_module
                .call_method1("zarr_has", (store_name, key))
                .unwrap();

            // Convert the Python coroutine into a Rust future
            pyo3_async_runtimes::tokio::into_future(coroutine)
        })
        .expect("Failed to create future")
        .await
        .expect("Failed to await future");

        Python::attach(|py| py_obj.bind(py).extract::<bool>())
            .expect("Failed to extract bool from Python object")
    }

    pub async fn zarr_get(store_name: &str, key: &str) -> zarrs::storage::Bytes {
        let py_obj = Python::attach(|py| {
            let zarr_module = PyModule::import(py, "pluot.zarr").unwrap();
            let coroutine = zarr_module
                .call_method1("zarr_get", (store_name, key))
                .unwrap();
            pyo3_async_runtimes::tokio::into_future(coroutine)
        })
        .expect("Failed to create future")
        .await
        .expect("Failed to await future");

        let result = Python::attach(|py| py_obj.bind(py).extract::<Vec<u8>>())
            .expect("Failed to extract bytes from Python object");
        zarrs::storage::Bytes::from(result)
    }

    pub async fn zarr_get_range_from_offset(
        store_name: &str,
        key: &str,
        offset: u32,
        length: u32,
    ) -> zarrs::storage::Bytes {
        let py_obj = Python::attach(|py| {
            let zarr_module = PyModule::import(py, "pluot.zarr").unwrap();
            let coroutine = zarr_module
                .call_method1(
                    "zarr_get_range_from_offset",
                    (store_name, key, offset, length),
                )
                .unwrap();
            pyo3_async_runtimes::tokio::into_future(coroutine)
        })
        .expect("Failed to create future")
        .await
        .expect("Failed to await future");

        let result = Python::attach(|py| py_obj.bind(py).extract::<Vec<u8>>())
            .expect("Failed to extract bytes from Python object");
        zarrs::storage::Bytes::from(result)
    }

    pub async fn zarr_get_range_from_end(
        store_name: &str,
        key: &str,
        suffix_length: u32,
    ) -> zarrs::storage::Bytes {
        let py_obj = Python::attach(|py| {
            let zarr_module = PyModule::import(py, "pluot.zarr").unwrap();
            let coroutine = zarr_module
                .call_method1("zarr_get_range_from_end", (store_name, key, suffix_length))
                .unwrap();
            pyo3_async_runtimes::tokio::into_future(coroutine)
        })
        .expect("Failed to create future")
        .await
        .expect("Failed to await future");

        let result = Python::attach(|py| py_obj.bind(py).extract::<Vec<u8>>())
            .expect("Failed to extract bytes from Python object");
        zarrs::storage::Bytes::from(result)
    }

    #[pyfunction]
    #[pyo3(signature = (screen_x, screen_y, **kwds))]
    pub fn pick_py(py: Python, screen_x: f32, screen_y: f32, kwds: Option<Py<PyAny>>) -> PyResult<Bound<PyAny>> {
        let params: RenderParams = if let Some(dict) = kwds {
            depythonize::<RenderParams>(&dict.into_bound(py)).unwrap()
        } else {
            RenderParams::default()
        };

        let screen_coord = ScreenCoord { x: screen_x, y: screen_y };

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = pick(params, screen_coord).await;
            Python::attach(|py| {
                pythonize::pythonize(py, &result)
                    .map(|v| v.unbind())
                    .map_err(|e| PyErr::from(e))
            })
        })
    }

    #[pyfunction]
    #[pyo3(signature = (**kwds))]
    pub fn render_py(py: Python, kwds: Option<Py<PyAny>>) -> PyResult<Bound<PyAny>> {
        // Use the py parameter directly instead of Python::with_gil
        let params: RenderParams = if let Some(dict) = kwds {
            depythonize::<RenderParams>(&dict.into_bound(py)).unwrap()
        } else {
            RenderParams::default()
        };

        pyo3_async_runtimes::tokio::future_into_py(py, async {
            let pixels = render(params).await;
            Ok(pixels)
        })
    }

    // This function creates the Python module.
    #[pymodule]
    fn _internal(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
        pyo3_log::init();
        let _ = Logger::new(_py, Caching::LoggersAndLevels)?.install();

        m.add_function(wrap_pyfunction!(log_info, m)?)?;
        m.add_function(wrap_pyfunction!(render_py, m)?)?;
        m.add_function(wrap_pyfunction!(pick_py, m)?)?;
        Ok(())
    }
}

// === R Bindings ===
#[cfg(all(not(target_arch = "wasm32"), feature = "rlang"))]
pub mod r {
    use extendr_api::prelude::*;
    use super::ZarrPeekResult;

    pub fn log(s: &str) {
        println!("{}", s);
    }

    fn i32_to_peek(val: i32) -> ZarrPeekResult {
        match val {
            1 => ZarrPeekResult::Fulfilled,
            2 => ZarrPeekResult::Rejected,
            _ => ZarrPeekResult::Pending,
        }
    }

    fn robj_to_bytes(r: Robj) -> Vec<u8> {
        Raw::try_from(r).map(|raw| raw.as_slice().to_vec()).unwrap_or_default()
    }

    pub fn zarr_has_status(store_name: &str, key: &str) -> ZarrPeekResult {
        i32_to_peek(
            R!("pluotr:::pluot_zarr_has_status({{store_name}}, {{key}})")
                .ok().and_then(|r| i32::try_from(r).ok()).unwrap_or(2)
        )
    }

    pub fn zarr_get_status(store_name: &str, key: &str) -> ZarrPeekResult {
        i32_to_peek(
            R!("pluotr:::pluot_zarr_get_status({{store_name}}, {{key}})")
                .ok().and_then(|r| i32::try_from(r).ok()).unwrap_or(2)
        )
    }

    pub fn zarr_get_range_from_offset_status(
        store_name: &str, key: &str, offset: u32, length: u32,
    ) -> ZarrPeekResult {
        let offset = offset as i32;
        let length = length as i32;
        i32_to_peek(
            R!("pluotr:::pluot_zarr_get_range_from_offset_status({{store_name}}, {{key}}, {{offset}}, {{length}})")
                .ok().and_then(|r| i32::try_from(r).ok()).unwrap_or(2)
        )
    }

    pub fn zarr_get_range_from_end_status(
        store_name: &str, key: &str, suffix_length: u32,
    ) -> ZarrPeekResult {
        let suffix_length = suffix_length as i32;
        i32_to_peek(
            R!("pluotr:::pluot_zarr_get_range_from_end_status({{store_name}}, {{key}}, {{suffix_length}})")
                .ok().and_then(|r| i32::try_from(r).ok()).unwrap_or(2)
        )
    }

    pub async fn zarr_has(store_name: &str, key: &str) -> bool {
        R!("pluotr:::pluot_zarr_has({{store_name}}, {{key}})")
            .ok().and_then(|r| i32::try_from(r).ok()).map(|v| v != 0).unwrap_or(false)
    }

    pub async fn zarr_get(store_name: &str, key: &str) -> zarrs::storage::Bytes {
        zarrs::storage::Bytes::from(
            R!("pluotr:::pluot_zarr_get({{store_name}}, {{key}})")
                .ok().map(robj_to_bytes).unwrap_or_default()
        )
    }

    pub async fn zarr_get_range_from_offset(
        store_name: &str, key: &str, offset: u32, length: u32,
    ) -> zarrs::storage::Bytes {
        let offset = offset as i32;
        let length = length as i32;
        zarrs::storage::Bytes::from(
            R!("pluotr:::pluot_zarr_get_range_from_offset({{store_name}}, {{key}}, {{offset}}, {{length}})")
                .ok().map(robj_to_bytes).unwrap_or_default()
        )
    }

    pub async fn zarr_get_range_from_end(
        store_name: &str, key: &str, suffix_length: u32,
    ) -> zarrs::storage::Bytes {
        let suffix_length = suffix_length as i32;
        zarrs::storage::Bytes::from(
            R!("pluotr:::pluot_zarr_get_range_from_end({{store_name}}, {{key}}, {{suffix_length}})")
                .ok().map(robj_to_bytes).unwrap_or_default()
        )
    }
}

// === Rust-only Bindings ===
#[cfg(all(not(target_arch = "wasm32"), not(feature = "python"), not(feature = "rlang")))]
pub mod plain_rust {
    use core::panic;
    pub use super::{render, ZarrPeekResult};

    pub fn log(s: &str) {
        println!("{}", s);
    }

    pub async fn zarr_has(store_name: &str, key: &str) -> bool {
        panic!("zarr_has is not implemented in plain Rust mode.");
    }

    pub fn zarr_has_status(store_name: &str, key: &str) -> ZarrPeekResult {
        panic!("zarr_has_status is not implemented in plain Rust mode.");
    }

    pub async fn zarr_get(store_name: &str, key: &str) -> zarrs::storage::Bytes {
        panic!("zarr_get is not implemented in plain Rust mode.");
    }

    pub fn zarr_get_status(store_name: &str, key: &str) -> ZarrPeekResult {
        panic!("zarr_get_status is not implemented in plain Rust mode.");
    }

    pub async fn zarr_get_range_from_offset(
        store_name: &str,
        key: &str,
        offset: u32,
        length: u32,
    ) -> zarrs::storage::Bytes {
        panic!("zarr_get_range_from_offset is not implemented in plain Rust mode.");
    }

    pub fn zarr_get_range_from_offset_status(
        store_name: &str,
        key: &str,
        offset: u32,
        length: u32,
    ) -> ZarrPeekResult {
        panic!("zarr_get_range_from_offset_status is not implemented in plain Rust mode.");
    }

    pub async fn zarr_get_range_from_end(
        store_name: &str,
        key: &str,
        suffix_length: u32,
    ) -> zarrs::storage::Bytes {
        panic!("zarr_get_range_from_end is not implemented in plain Rust mode.");
    }

    pub fn zarr_get_range_from_end_status(
        store_name: &str,
        key: &str,
        suffix_length: u32,
    ) -> ZarrPeekResult {
        panic!("zarr_get_range_from_end is not implemented in plain Rust mode.");
    }

}
