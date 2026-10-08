//! Opens the OS window and draws into it.

use std::sync::Arc;

use crate::{
    host::{App, Host},
    platform::gpu::{self, Gpu},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    EventLoop(#[from] winit::error::EventLoopError),

    #[error("couldn't open a window: {0}")]
    Os(#[from] winit::error::OsError),

    #[error(transparent)]
    Gpu(#[from] gpu::Error),

    // `Box<dyn Error>` doesn't implement `Error`, so it can't be a source.
    #[error("{0}")]
    App(Box<dyn std::error::Error>),
}

/// Opens the window and blocks until it's closed.
///
/// # Errors
///
/// When the app can't start, the window or its GPU surface can't be opened,
/// or the surface is lost.
pub fn run<A: App>() -> Result<(), Error> {
    let mut runner = Runner {
        host: Host::<A>::new().map_err(Error::App)?,
        open: None,
        error: None,
    };
    EventLoop::new()?.run_app(&mut runner)?;
    runner.error.map_or(Ok(()), Err)
}

struct Runner<A> {
    host: Host<A>,
    open: Option<Open>,
    // winit's callbacks can't return one, so it waits here for run.
    error: Option<Error>,
}

struct Open {
    window: Arc<Window>,
    gpu: Gpu,
}

impl Open {
    fn new(event_loop: &ActiveEventLoop, title: &str) -> Result<Self, Error> {
        let window =
            Arc::new(event_loop.create_window(Window::default_attributes().with_title(title))?);
        let size = window.inner_size();
        let gpu = Gpu::new(Arc::clone(&window), [size.width, size.height])?;
        Ok(Self { window, gpu })
    }
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
            WindowEvent::Resized(size) => {
                open.gpu.resize([size.width, size.height]);
                open.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let background = self.host.draw();
                if let Err(error) = open.gpu.draw(background) {
                    self.fail(event_loop, error.into());
                }
            }
            _ => {}
        }
    }

    // Every way out passes through here, including a failure.
    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.host.close();
    }
}
