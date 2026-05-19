use futures::executor::block_on;
use pluot::RenderParams;
use pluot::render as pluot_render;

pub(crate) fn do_render(json_str: &str) -> Result<Vec<u8>, String> {
    let params: RenderParams = serde_json::from_str(json_str)
        .map_err(|e| format!("pluot: failed to parse RenderParams: {e}"))?;
    Ok(block_on(pluot_render(params)))
}
