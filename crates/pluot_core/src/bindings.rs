pub use crate::params::RenderParams;
pub use crate::render::render;
pub use crate::picking::{pick, PickingResult};
pub use crate::viewport::ScreenCoord;

// == WASM Bindings ===
#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use super::{render, pick, RenderParams, ScreenCoord};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console)]
        pub fn log(s: &str);

        // We need to define a `has` function, since the zarr_get_js function
        // may return undefined, and wasm_bindgen does not allow
        // Option<Uint8Array> as a return type annotation.
        #[wasm_bindgen(js_name = zarr_has)]
        async fn zarr_has_js(store_name: &str, key: &str) -> js_sys::Boolean;

        #[wasm_bindgen(js_name = zarr_get)]
        async fn zarr_get_js(store_name: &str, key: &str) -> js_sys::Uint8Array;

        #[wasm_bindgen(js_name = zarr_get_range_from_offset)]
        async fn zarr_get_range_from_offset_js(
            store_name: &str,
            key: &str,
            offset: u32,
            length: u32,
        ) -> js_sys::Uint8Array;

        #[wasm_bindgen(js_name = zarr_get_range_from_end)]
        async fn zarr_get_range_from_end_js(
            store_name: &str,
            key: &str,
            suffix_length: u32,
        ) -> js_sys::Uint8Array;
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

    pub async fn zarr_get(store_name: &str, key: &str) -> zarrs::storage::Bytes {
        let js_bytes = zarr_get_js(store_name, key).await;
        convert_to_bytes(js_bytes)
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

    pub async fn zarr_get_range_from_end(
        store_name: &str,
        key: &str,
        suffix_length: u32,
    ) -> zarrs::storage::Bytes {
        let js_bytes = zarr_get_range_from_end_js(store_name, key, suffix_length).await;
        convert_to_bytes(js_bytes)
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

    use super::{render, pick, RenderParams, ScreenCoord};

    #[pyfunction]
    pub fn log_info(s: &str) {
        info!("{}", s);
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
            Python::attach(|py| pythonize::pythonize(py, &result).map_err(|e| e.into()))
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

// === Rust-only Bindings ===
#[cfg(all(not(target_arch = "wasm32"), not(feature = "python")))]
pub mod plain_rust {
    use core::panic;

    pub use super::render;

    pub fn log(s: &str) {
        println!("{}", s);
    }

    pub async fn zarr_has(store_name: &str, key: &str) -> bool {
        panic!("zarr_has is not implemented in plain Rust mode.");
    }

    pub async fn zarr_get(store_name: &str, key: &str) -> zarrs::storage::Bytes {
        panic!("zarr_get is not implemented in plain Rust mode.");
    }

    pub async fn zarr_get_range_from_offset(
        store_name: &str,
        key: &str,
        offset: u32,
        length: u32,
    ) -> zarrs::storage::Bytes {
        panic!("zarr_get_range_from_offset is not implemented in plain Rust mode.");
    }

    pub async fn zarr_get_range_from_end(
        store_name: &str,
        key: &str,
        suffix_length: u32,
    ) -> zarrs::storage::Bytes {
        panic!("zarr_get_range_from_end is not implemented in plain Rust mode.");
    }
}
