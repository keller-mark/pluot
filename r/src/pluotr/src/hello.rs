use std::ffi::{CString, CStr};
use std::os::raw::c_char;
use futures::executor::block_on;

extern "C" {
    fn call_r_info_helper() -> *const c_char;
}

async fn get_string_async() -> String {
    "Hello ピカチュウ async !".to_string()
}


#[no_mangle]
pub extern "C" fn rust_roundtrip() -> *const c_char {
    let r_str_ptr = unsafe { call_r_info_helper() };
    let r_str = unsafe { CStr::from_ptr(r_str_ptr) }.to_str().unwrap_or("");
    let rust_str = block_on(get_string_async());
    let new_string = format!("{} + {}", r_str, rust_str);
    let s = CString::new(new_string).unwrap();
    s.into_raw()
}

#[no_mangle]
pub extern "C" fn free_string_from_rust(ptr: *mut c_char) {
    let _ = unsafe { CString::from_raw(ptr) };
}
