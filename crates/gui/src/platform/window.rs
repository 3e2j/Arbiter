//! Opens the OS window and draws into it.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use super::pacer::Pacer;
use crate::{
    canvas::{Canvas, Rect},
    cast::narrow,
    host::{App, Host},
    render::{self, Gpu},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

/// Used when the monitor doesn't report its refresh rate.
const FALLBACK_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    EventLoop(#[from] winit::error::EventLoopError),

    #[error("couldn't open a window: {0}")]
    Os(#[from] winit::error::OsError),

    #[error(transparent)]
    Gpu(#[from] render::Error),

    // `Box<dyn Error>` doesn't implement `Error`, so it can't be a source.
    #[error("{0}")]
    App(Box<dyn std::error::Error>),
}

/// Opens the window and blocks until it's closed.
///
/// # Errors
///
/// When the app can't start, or the window or its GPU surface can't be opened,
/// including again after the surface is lost.
pub fn run<A: App>() -> Result<(), Error> {
    let mut runner = Runner {
        host: Host::<A>::new().map_err(Error::App)?,
        canvas: Canvas::default(),
        open: None,
        error: None,
    };
    EventLoop::new()?.run_app(&mut runner)?;
    runner.error.map_or(Ok(()), Err)
}

struct Runner<A> {
    host: Host<A>,
    // Kept between passes so a warm pass allocates nothing.
    canvas: Canvas,
    open: Option<Open>,
    // winit's callbacks can't return one, so it waits here for run.
    error: Option<Error>,
}

struct Open {
    window: Arc<Window>,
    gpu: Gpu,
    pacer: Pacer,
}

impl Open {
    fn new(event_loop: &ActiveEventLoop, title: &str) -> Result<Self, Error> {
        let window =
            Arc::new(event_loop.create_window(Window::default_attributes().with_title(title))?);
        let gpu = gpu(&window)?;
        let pacer = Pacer::new((!compositor_paced(event_loop)).then(|| interval(&window)));
        Ok(Self { window, gpu, pacer })
    }

    /// The window's contents in logical pixels.
    fn rect(&self) -> Rect {
        let size = self
            .window
            .inner_size()
            .to_logical::<f32>(self.window.scale_factor());
        Rect::new(0., 0., size.width, size.height)
    }
}

/// A new surface on `window`, at its current size and scale.
fn gpu(window: &Arc<Window>) -> Result<Gpu, render::Error> {
    let size = window.inner_size();
    Gpu::new(
        // The surface holds a clone, so the window can't close before the surface is dropped.
        Arc::clone(window),
        [size.width, size.height],
        window.scale_factor(),
    )
}

impl<A> Runner<A> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: Error) {
        self.error = Some(error);
        event_loop.exit();
    }
}

impl<A: App> ApplicationHandler for Runner<A> {
    // Fires once at startup on desktop.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.open.is_some() {
            return;
        }
        match Open::new(event_loop, A::TITLE) {
            Ok(open) => self.open = Some(open),
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(open) = &mut self.open else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            // A drag sends many of these per refresh. The pacer turns them into one
            // redraw, which resizes the surface to the latest size.
            WindowEvent::Resized(_) => open.pacer.request(),
            WindowEvent::ScaleFactorChanged { .. } => {
                open.pacer.set_interval(interval(&open.window));
                open.pacer.request();
            }
            WindowEvent::Moved(_) => open.pacer.set_interval(interval(&open.window)),
            WindowEvent::RedrawRequested => {
                open.pacer.drew(Instant::now());
                let size = open.window.inner_size();
                open.gpu
                    .resize([size.width, size.height], open.window.scale_factor());
                let scale = narrow(open.window.scale_factor());
                self.host.draw(open.rect(), scale, &mut self.canvas);
                let atlas = self.host.take_atlas_update();
                // Without this, Wayland gets a frame per event and shows them all in turn.
                let window = &open.window;
                match open
                    .gpu
                    .draw(&self.canvas, atlas, || window.pre_present_notify())
                {
                    Ok(()) => {}
                    Err(render::Error::Lost) => match gpu(&open.window) {
                        Ok(gpu) => {
                            open.gpu = gpu;
                            self.host.reupload_atlas();
                            open.pacer.request();
                        }
                        Err(error) => self.fail(event_loop, error.into()),
                    },
                    Err(error) => self.fail(event_loop, error.into()),
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(open) = &mut self.open else { return };
        let (redraw, due) = open.pacer.poll(Instant::now());
        if redraw {
            open.window.request_redraw();
        }
        event_loop.set_control_flow(due.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    // Every way out passes through here, including a failure.
    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.host.close();
    }
}

/// The time between refreshes of the monitor `window` is on.
fn interval(window: &Window) -> Duration {
    window
        .current_monitor()
        .and_then(|monitor| monitor.refresh_rate_millihertz())
        .filter(|&millihertz| millihertz > 0)
        .map_or(FALLBACK_INTERVAL, |millihertz| {
            Duration::from_nanos(1_000_000_000_000 / u64::from(millihertz))
        })
}

/// Whether the compositor holds each redraw until it has shown the last, which
/// winit does on Wayland once [`Window::pre_present_notify`] is called.
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
))]
fn compositor_paced(event_loop: &ActiveEventLoop) -> bool {
    use winit::platform::wayland::ActiveEventLoopExtWayland;
    event_loop.is_wayland()
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
)))]
const fn compositor_paced(_: &ActiveEventLoop) -> bool {
    false
}
