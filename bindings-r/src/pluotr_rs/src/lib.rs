use extendr_api::prelude::*;

mod render;

#[extendr]
fn render_r(json_params: &str) -> Raw {
    match render::do_render(json_params) {
        Ok(bytes) => Raw::from_bytes(&bytes),
        Err(e) => panic!("{e}"),
    }
}

#[extendr]
fn render_to_script_r(json_params: &str) -> String {
    match render::do_render_to_script(json_params) {
        Ok(code_string) => code_string,
        Err(e) => panic!("{e}"),
    }
}


extendr_module! {
    mod pluotr;
    fn render_r;
}
