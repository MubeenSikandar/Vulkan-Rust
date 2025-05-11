use crate::app::App;
use anyhow::Result;
use winit::event_loop::EventLoop;

mod app;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let mut app = App::default();
    let event_loop = EventLoop::with_user_event().build().unwrap();

    event_loop.run_app(&mut app)?;

    Ok(())
}
