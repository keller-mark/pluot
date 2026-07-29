use futures::executor::block_on;
use pluot::{RenderParams, CodeFormat, render as pluot_render, render_to_script as pluot_render_to_script};

pub(crate) fn do_render(json_str: &str) -> Result<Vec<u8>, String> {
    let params: RenderParams = serde_json::from_str(json_str)
        .map_err(|e| format!("pluot: failed to parse RenderParams: {e}"))?;
    Ok(block_on(pluot_render(params)))
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
