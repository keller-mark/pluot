use crate::wgpu;
use crate::zarr::AsyncZarritaStore;
use crate::layer_traits::AspectRatioMode;
use serde::{Deserialize, Serialize};
use svg::node::element::Group;
use std::sync::Arc;

use crate::params::RenderParams;

pub struct GpuContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
}

pub struct RenderAndComputeContext<'a> {
    pub params: &'a RenderParams,
    pub store: &'a Arc<AsyncZarritaStore>,

    pub gpu_context: Option<GpuContext<'a>>,

    // pub out_tex: &'a wgpu::Texture,
}

pub struct PrepareResult {
    // Whether this layer bailed early due to the provided timeout.
    pub bailed_early: bool,
    // TODO: do we need a `timeout_remaining` field here to track the time remaining for subsequent layers
    // after earlier layers have used up a portion of the timeout budget? Or, can we just use maybe_timeout!
    // on joined futures to handle this instead?
}

pub struct RenderResult {
    // Whether one or more layers bailed early due to the provided timeout.
    // Only relevant in interactive settings.
    // In non-interactive settings, timeout will be None, so this should always be false.
    pub bailed_early: bool,
}
