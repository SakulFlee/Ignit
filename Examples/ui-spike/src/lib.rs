//! Throwaway UI foundation spike. Not shipped, not intended to merge.

pub mod app;
pub mod draw;
pub mod layout;
pub mod render;
pub mod text;

pub use app::SpikeModule;

#[ignit::main]
pub fn entrypoint(
    event_loop_result: Result<
        ignit::winit::event_loop::EventLoop<()>,
        ignit::winit::error::EventLoopError,
    >,
) {
    let event_loop = event_loop_result.expect("event loop");
    let settings = ignit::app::AppSettings {
        fullscreen: false,
        name: "Ignit UI Spike".into(),
        ..Default::default()
    };
    ignit::app::App::new()
        .add_module(SpikeModule)
        .liftoff(event_loop, settings)
        .expect("runtime");
}
