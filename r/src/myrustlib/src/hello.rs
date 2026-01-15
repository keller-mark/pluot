use std::ffi::CString;
use std::os::raw::c_char;
use futures::executor::block_on;

async fn get_string_async() -> String {
    "Hello ピカチュウ async !".to_string()
}

#[no_mangle]
pub extern "C" fn string_from_rust_async() -> *const c_char {
    let s_str = block_on(get_string_async());
    let s = CString::new(s_str).unwrap();
    s.into_raw()
}

#[no_mangle]
pub extern "C" fn free_string_from_rust(ptr: *mut c_char) {
    let _ = unsafe { CString::from_raw(ptr) };
}
