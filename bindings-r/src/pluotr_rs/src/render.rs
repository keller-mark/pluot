use futures::executor::block_on;
use pluot::{
    RenderParams, CodeFormat, ViewParams, MarginParams, DataBounds, get_camera_matrix_from_bounds,
    render as pluot_render, extent as pluot_extent, render_to_script as pluot_render_to_script,
};

fn parse_render_params(json_str: &str) -> Result<RenderParams, String> {
    serde_json::from_str(json_str)
        .map_err(|e| format!("pluot: failed to parse RenderParams: {e}"))
}

pub(crate) fn do_render(json_str: &str) -> Result<Vec<u8>, String> {
    let params = parse_render_params(json_str)?;
    Ok(block_on(pluot_render(params)))
}

pub(crate) fn do_extent(json_str: &str) -> Result<String, String> {
    let params = parse_render_params(json_str)?;
    let result = block_on(pluot_extent(params));
    serde_json::to_string(&result)
        .map_err(|e| format!("pluot: failed to serialize ExtentResult: {e}"))
}

pub(crate) fn do_camera_view_from_lims(json_str: &str, x_lim: (f32, f32), y_lim: (f32, f32)) -> Result<[f32; 16], String> {
    let params = parse_render_params(json_str)?;
    let view_params = ViewParams {
        width: params.width,
        height: params.height,
        aspect_ratio_mode: params.aspect_ratio_mode,
        aspect_ratio_alignment_mode: params.aspect_ratio_alignment_mode,
        margins: Some(MarginParams {
            margin_top: params.margin_top,
            margin_right: params.margin_right,
            margin_bottom: params.margin_bottom,
            margin_left: params.margin_left,
        }),
        ..Default::default()
    };
    let data_bounds = DataBounds {
        x_min: x_lim.0,
        x_max: x_lim.1,
        y_min: y_lim.0,
        y_max: y_lim.1,
    };
    Ok(get_camera_matrix_from_bounds(&view_params, &data_bounds))
}

pub(crate) fn do_render_to_script(json_str: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("pluot: failed to parse RenderParams: {e}"))?;

    let params: RenderParams = serde_json::from_value(value.clone())
        .map_err(|e| format!("pluot: failed to parse RenderParams: {e}"))?;

    let code_format: CodeFormat = value
        .get("code_format")
        .cloned()
        .ok_or_else(|| "pluot: missing required 'code_format' field".to_string())
        .and_then(|v| serde_json::from_value(v)
            .map_err(|e| format!("pluot: failed to parse code_format: {e}")))?;

    Ok(pluot_render_to_script(params, &code_format))
}
