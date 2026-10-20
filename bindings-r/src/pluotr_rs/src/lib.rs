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
fn extent_r(json_params: &str) -> String {
    match render::do_extent(json_params) {
        Ok(json_result) => json_result,
        Err(e) => panic!("{e}"),
    }
}

#[extendr]
fn camera_view_from_lims_r(json_params: &str, x_lim: &[f64], y_lim: &[f64]) -> Vec<f64> {
    let as_pair = |name: &str, lim: &[f64]| match lim {
        [min, max] => (*min as f32, *max as f32),
        _ => panic!("pluot: `{name}` must be a numeric vector of length 2"),
    };
    let x_lim = as_pair("x_lim", x_lim);
    let y_lim = as_pair("y_lim", y_lim);
    match render::do_camera_view_from_lims(json_params, x_lim, y_lim) {
        Ok(camera_view) => camera_view.iter().map(|v| *v as f64).collect(),
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
    fn extent_r;
    fn camera_view_from_lims_r;
}
