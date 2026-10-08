//! Draws into any surface wgpu accepts, and never sees a window.
//!
//! For now a draw only clears the surface.

use wgpu::{CurrentSurfaceTexture, SurfaceTarget};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("couldn't create a surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),

    #[error("no graphics adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),

    #[error("couldn't open the graphics device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),

    #[error("the graphics adapter can't present to this surface")]
    Unsupported,

    #[error("the surface was lost")]
    Lost,
}

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

impl Gpu {
    /// Blocks until the device is ready.
    ///
    /// # Errors
    ///
    /// When there's no adapter or device that can present to `target`.
    pub fn new(target: impl Into<SurfaceTarget<'static>>, size: [u32; 2]) -> Result<Self, Error> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(target)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
        // wgpu rejects a zero-sized surface, which a window can report before it's mapped.
        let [width, height] = size.map(|n| n.max(1));
        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or(Error::Unsupported)?;
        surface.configure(&device, &config);
        Ok(Self {
            surface,
            device,
            queue,
            config,
        })
    }

    /// A zero size, such as a minimized window, keeps the last one.
    pub fn resize(&mut self, [width, height]: [u32; 2]) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Fills the surface with `background`, as linear RGBA. Skips the frame
    /// when the surface isn't ready, such as while it's hidden.
    ///
    /// # Errors
    ///
    /// [`Error::Lost`] when the surface is gone and needs a new [`Gpu`].
    // TODO: take `&Canvas` from `gui::canvas` and draw its quads over the clear.
    pub fn draw(&mut self, background: [f64; 4]) -> Result<(), Error> {
        let (frame, suboptimal) = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) => (frame, false),
            CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return Ok(()),
            CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            CurrentSurfaceTexture::Lost | CurrentSurfaceTexture::Validation => {
                return Err(Error::Lost);
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let [r, g, b, a] = background;
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        if suboptimal {
            self.surface.configure(&self.device, &self.config);
        }
        Ok(())
    }
}
