use std::borrow::Cow;

use crate::{utils::RenderContext};

pub async fn render_bioimage(context: &RenderContext<'_>, encoder: &mut wgpu::CommandEncoder) {
    // Get x and y data from the Zarr store.
    let store = context.store;

    // TODO: implement bioimage rendering.

}