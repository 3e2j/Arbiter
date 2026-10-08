//! Draws a [`Canvas`] into any surface wgpu accepts, and never sees a window.
//!
//! Every quad is one instance of a 4-vertex strip, and a new draw call starts
//! only where the clip changes.

use std::ops::Range;

use wgpu::{CurrentSurfaceTexture, SurfaceTarget};

use crate::canvas::{Canvas, Quad, Rect};

const QUAD_SIZE: wgpu::BufferAddress = size_of::<Quad>() as wgpu::BufferAddress;

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

    #[error("too many quads to draw in one pass")]
    TooManyQuads,
}

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// Physical pixels per logical one.
    scale: f64,
    pipeline: wgpu::RenderPipeline,
    viewport: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Grows to the largest canvas drawn so far, and never shrinks.
    instances: wgpu::Buffer,
}

impl Gpu {
    /// Blocks until the device is ready. `scale` is physical pixels per
    /// logical one.
    ///
    /// # Errors
    ///
    /// When there's no adapter or device that can present to `target`.
    pub fn new(
        target: impl Into<SurfaceTarget<'static>>,
        size: [u32; 2],
        scale: f64,
    ) -> Result<Self, Error> {
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

        let viewport = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport"),
            layout: &viewport_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport.as_entire_binding(),
            }],
        });
        let pipeline = quad_pipeline(&device, &viewport_layout, config.format);
        let instances = instance_buffer(&device, 1);

        let gpu = Self {
            surface,
            device,
            queue,
            config,
            scale,
            pipeline,
            viewport,
            bind_group,
            instances,
        };
        gpu.write_viewport();
        Ok(gpu)
    }

    /// A zero size, such as a minimized window, keeps the last one.
    pub fn resize(&mut self, [width, height]: [u32; 2], scale: f64) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.scale = scale;
        self.surface.configure(&self.device, &self.config);
        self.write_viewport();
    }

    /// Clears the surface to the canvas's background and draws its quads.
    /// Skips the frame when the surface isn't ready, such as while it's hidden.
    ///
    /// # Errors
    ///
    /// [`Error::Lost`] when the surface is gone and needs a new [`Gpu`], and
    /// [`Error::TooManyQuads`] when the canvas can't fit in one buffer.
    pub fn draw(&mut self, canvas: &Canvas) -> Result<(), Error> {
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
        self.upload(canvas.quads())?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let [r, g, b, a] = canvas.background.0.map(f64::from);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            for (clip, quads) in canvas.batches() {
                let Some([left, top, width, height]) = self.scissor(clip) else {
                    continue;
                };
                pass.set_scissor_rect(left, top, width, height);
                pass.draw(0..4, instance_range(quads)?);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        if suboptimal {
            self.surface.configure(&self.device, &self.config);
        }
        Ok(())
    }

    fn write_viewport(&self) {
        let size =
            [self.config.width, self.config.height].map(|n| narrow(f64::from(n) / self.scale));
        let viewport = [size[0], size[1], 0., 0.];
        self.queue
            .write_buffer(&self.viewport, 0, bytemuck::cast_slice(&viewport));
    }

    fn upload(&mut self, quads: &[Quad]) -> Result<(), Error> {
        if quads.is_empty() {
            return Ok(());
        }
        let bytes: &[u8] = bytemuck::cast_slice(quads);
        let size = wgpu::BufferAddress::try_from(bytes.len()).map_err(|_| Error::TooManyQuads)?;
        if size > self.instances.size() {
            if size > self.device.limits().max_buffer_size {
                return Err(Error::TooManyQuads);
            }
            self.instances =
                instance_buffer(&self.device, size.div_ceil(QUAD_SIZE).next_power_of_two());
        }
        self.queue.write_buffer(&self.instances, 0, bytes);
        Ok(())
    }

    /// `clip` in physical pixels, cut to the surface. `None` when nothing of it is left.
    fn scissor(&self, clip: Rect) -> Option<[u32; 4]> {
        let surface = [self.config.width, self.config.height];
        let to_pixel = |v: f32, max: u32| pixel(f64::from(v) * self.scale).min(max);
        let x = to_pixel(clip.x, surface[0]);
        let y = to_pixel(clip.y, surface[1]);
        let right = to_pixel(clip.right(), surface[0]);
        let bottom = to_pixel(clip.bottom(), surface[1]);
        (right > x && bottom > y).then(|| [x, y, right - x, bottom - y])
    }
}

fn instance_buffer(device: &wgpu::Device, quads: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("quads"),
        size: quads * QUAD_SIZE,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn instance_range(quads: Range<usize>) -> Result<Range<u32>, Error> {
    let start = u32::try_from(quads.start).map_err(|_| Error::TooManyQuads)?;
    let end = u32::try_from(quads.end).map_err(|_| Error::TooManyQuads)?;
    Ok(start..end)
}

fn quad_pipeline(
    device: &wgpu::Device,
    viewport_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("quad.wgsl"));
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("quad"),
        bind_group_layouts: &[Some(viewport_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("quad"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: QUAD_SIZE,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32,
                ],
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

// wgpu takes f32 positions and u32 pixels, and Rust has no conversion into
// either from f64 that isn't `as`. Both saturate, which is what's wanted here.
#[allow(clippy::cast_possible_truncation)]
fn narrow(v: f64) -> f32 {
    v as f32
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn pixel(v: f64) -> u32 {
    v.round() as u32
}
