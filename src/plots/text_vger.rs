use std::cell::RefCell;
use std::sync::Arc;

use crate::wgpu;
use vger::Vger;

thread_local! {
    static VGER_RENDERER: RefCell<Option<Vger>> = RefCell::new(None);
}

//const FONT_BYTES: &[u8] = include_bytes!("fonts/Inter-Bold.ttf").as_slice();

#[cfg(target_arch = "wasm32")]
pub fn with_vger_renderer<F, R>(device: &wgpu::Device, queue: &wgpu::Queue, f: F) -> R
where
    F: FnOnce(&mut Vger) -> R,
{
    VGER_RENDERER.with(|renderer| {
        // Check if already initialized
        if renderer.borrow().is_none() {
            let mut vger_renderer = Vger::new(
                Arc::new(device.clone()).clone(),
                Arc::new(queue.clone()).clone(),
                wgpu::TextureFormat::Rgba8UnormSrgb,
            );

            //let settings = fontdue::FontSettings::default();
            //vger_renderer.glyph_cache.font = fontdue::Font::from_bytes(FONT_BYTES, settings).unwrap();

            *renderer.borrow_mut() = Some(vger_renderer);
        }

        f(renderer.borrow_mut().as_mut().unwrap())
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn with_vger_renderer<F, R>(device: &wgpu::Device, queue: &wgpu::Queue, f: F) -> R
where
    F: FnOnce(&mut Vger) -> R,
{
    let mut vger_renderer = Vger::new(
        Arc::new(device.clone()).clone(),
        Arc::new(queue.clone()).clone(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );

    //let settings = fontdue::FontSettings::default();
    //vger_renderer.glyph_cache.font = fontdue::Font::from_bytes(FONT_BYTES, settings).unwrap();
    
    f(&mut vger_renderer)
}