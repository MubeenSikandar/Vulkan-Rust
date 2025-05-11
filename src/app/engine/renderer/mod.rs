use anyhow::Result;
use context::Context;
use std::sync::Arc;
use winit::window::Window;

mod context;

pub struct Renderer {
    context: Context,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Result<Self> {
        // Convert Arc<Window> to &Window (you can clone or use the reference)
        // SAFETY: Context::create is unsafe and requires caller to uphold Vulkan usage invariants
        let context = unsafe { Context::create(&window)? };

        Ok(Self { context })
    }
}
