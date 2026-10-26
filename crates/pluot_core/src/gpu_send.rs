//! Manual `Send`/`Sync` impls for the wgpu handles that layer futures capture.
//!
//! Motivation: compile time. On native targets, the layer traits use `#[async_trait]`, which
//! requires every layer future to be `Send` (needed by `pyo3_async_runtimes::future_into_py`).
//! rustc proves `Send` for a future structurally, so any wgpu handle captured by (or held across
//! an `.await` in) a layer method causes rustc to walk wgpu-core's entire internal type graph
//! (pipelines, bind group layouts, naga shader metadata, ...). rustc does not reuse that proof
//! across coroutine bodies, so it was repeated for every async layer method, which made
//! `pluot_core` and `pluot_zarr` take minutes to compile (mostly in MIR borrowck and
//! monomorphization collection) and could even exceed the default recursion limit.
//!
//! Implementing `Send`/`Sync` explicitly on these local types stops the structural walk here.
//! This is sound because wgpu itself asserts that these types are `Send + Sync` on native
//! targets; the assertions below re-check that once, so a wgpu change that broke it would fail
//! to compile rather than silently becoming unsound.
//!
//! Wrapping isn't practical for every wgpu value (e.g., the buffers and texture views that the
//! GPU compute functions in `compute` keep alive across `.await` points while waiting for
//! read-back). Those functions instead return a [`MaybeSendBoxFuture`]: the `Send` proof for a
//! boxed `dyn Future + Send` happens once, where the future is created, rather than being
//! repeated by every caller whose own future would otherwise contain this one.
//!
//! When adding async layer methods or GPU helpers, prefer passing these wrappers (or boxing the
//! future) over capturing raw wgpu types across `.await` points.

use crate::render_types::GpuContext;
use crate::wgpu;

/// A boxed future that is `Send` on native targets (where layer futures must be `Send`)
/// and not on wasm32 (where wgpu types are not `Send`).
#[cfg(not(target_arch = "wasm32"))]
pub type MaybeSendBoxFuture<'a, T> = futures::future::BoxFuture<'a, T>;
#[cfg(target_arch = "wasm32")]
pub type MaybeSendBoxFuture<'a, T> = futures::future::LocalBoxFuture<'a, T>;

/// Wrapper around [`wgpu::RenderPass`] that derefs to it. See the module docs for why it exists.
pub struct GpuRenderPass<'a>(pub wgpu::RenderPass<'a>);

impl<'a> std::ops::Deref for GpuRenderPass<'a> {
    type Target = wgpu::RenderPass<'a>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> std::ops::DerefMut for GpuRenderPass<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
const _: () = {
    fn assert_send_sync<T: Send + Sync>() {}
    fn assert_wrapped_wgpu_types_are_send_sync() {
        assert_send_sync::<wgpu::Device>();
        assert_send_sync::<wgpu::Queue>();
        assert_send_sync::<wgpu::RenderPass<'static>>();
    }
};

#[cfg(not(target_arch = "wasm32"))]
unsafe impl Send for GpuContext<'_> {}
#[cfg(not(target_arch = "wasm32"))]
unsafe impl Sync for GpuContext<'_> {}

#[cfg(not(target_arch = "wasm32"))]
unsafe impl Send for GpuRenderPass<'_> {}
#[cfg(not(target_arch = "wasm32"))]
unsafe impl Sync for GpuRenderPass<'_> {}
