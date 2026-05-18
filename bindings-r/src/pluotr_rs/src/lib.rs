use extendr_api::prelude::*;

mod render;

#[extendr]
fn render_r(json_params: &str) -> Raw {
    match render::do_render(json_params) {
        Ok(bytes) => Raw::from_bytes(&bytes),
        Err(e) => panic!("{e}"),
    }
}

extendr_module! {
    mod pluotr;
    fn render_r;
}
