use std::ffi::CStr;
use std::os::raw::c_char;
use futures::executor::block_on;
use pluot::RenderParams;
use pluot::render as pluot_render;

/// Render from a JSON string of RenderParams. Returns a heap-allocated byte
/// buffer and writes its length to `out_len`. Caller must free via
/// `free_bytes_from_rust`. Returns null on parse error.
#[no_mangle]
pub extern "C" fn rust_render(
    json_params: *const c_char,
    out_len: *mut usize,
) -> *mut u8 {
    let json_str = match unsafe { CStr::from_ptr(json_params) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let params: RenderParams = match serde_json::from_str(json_str) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("pluot: failed to parse RenderParams: {e}");
            return std::ptr::null_mut();
        }
    };
    let bytes = block_on(pluot_render(params));
    unsafe { *out_len = bytes.len(); }
    let mut boxed = bytes.into_boxed_slice();
    let ptr = boxed.as_mut_ptr();
    std::mem::forget(boxed);
    ptr
}

#[no_mangle]
pub extern "C" fn free_bytes_from_rust(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        unsafe {
            let _ = Box::from_raw(std::slice::from_raw_parts_mut(ptr, len));
        }
    }
}
