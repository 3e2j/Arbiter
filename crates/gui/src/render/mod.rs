//! Draws a [`Canvas`] into any surface wgpu accepts, and never sees a window.
//!
//! Every quad is one instance of a 4-vertex strip, and triangles are an indexed
//! list drawn by a second pipeline. A new draw call starts only where the clip
//! or the kind of shape changes. Text and icons read the atlas pages, layers
//! of a texture array per atlas that take only the rows that changed each
//! frame.

use std::ops::Range;

use wgpu::{CurrentSurfaceTexture, SurfaceTarget};

use crate::canvas::{AtlasUpdate, Canvas, Format, Kind, Quad, Rect, Vertex};
use crate::cast::{narrow, pixel};

const QUAD_SIZE: wgpu::BufferAddress = size_of::<Quad>() as wgpu::BufferAddress;
const VERTEX_SIZE: wgpu::BufferAddress = size_of::<Vertex>() as wgpu::BufferAddress;

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

    #[error("too many shapes to draw in one pass")]
    TooManyShapes,
}

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// Physical pixels per logical one.
    scale: f64,
    quad_pipeline: wgpu::RenderPipeline,
    triangle_pipeline: wgpu::RenderPipeline,
    viewport: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// These three grow to the largest canvas drawn so far, and never shrink.
    instances: wgpu::Buffer,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    atlas: Atlas,
}

/// Each atlas's pages, indexed by [`Format`], and the bind group that reads
/// them. An atlas's texture is replaced when a page is added, and the bind
/// group with it.
struct Atlas {
    layout: wgpu::BindGroupLayout,
    textures: [wgpu::Texture; 2],
    bind_group: wgpu::BindGroup,
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
        // The default takes the driver's first present mode, which can be Mailbox or
        // Immediate, so vsync is set here. Every backend must support Fifo.
        let config = wgpu::SurfaceConfiguration {
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            ..surface
                .get_default_config(&adapter, width, height)
                .ok_or(Error::Unsupported)?
        };
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
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
        let atlas = Atlas::new(&device);
        let quad_pipeline =
            quad_pipeline(&device, [&viewport_layout, &atlas.layout], config.format);
        let triangle_pipeline = triangle_pipeline(&device, &viewport_layout, config.format);
        let instances = buffer(&device, "quads", wgpu::BufferUsages::VERTEX, QUAD_SIZE);
        let vertices = buffer(&device, "vertices", wgpu::BufferUsages::VERTEX, VERTEX_SIZE);
        let indices = buffer(&device, "indices", wgpu::BufferUsages::INDEX, 4);

        let gpu = Self {
            surface,
            device,
            queue,
            config,
            scale,
            quad_pipeline,
            triangle_pipeline,
            viewport,
            bind_group,
            instances,
            vertices,
            indices,
            atlas,
        };
        gpu.write_viewport();
        Ok(gpu)
    }

    /// Reconfigures the surface when the size or scale changed. A zero size,
    /// such as a minimized window, keeps the last one.
    pub fn resize(&mut self, [width, height]: [u32; 2], scale: f64) {
        let same = [width, height] == [self.config.width, self.config.height]
            && scale.total_cmp(&self.scale).is_eq();
        if same || width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.scale = scale;
        self.surface.configure(&self.device, &self.config);
        self.write_viewport();
    }

    /// Uploads what changed in the atlases, then clears the surface to the
    /// canvas's background and draws its shapes, calling `before_present` right
    /// before the frame goes out.
    ///
    /// Skips the frame, without calling it, when the surface isn't ready, such
    /// as while it's hidden, but still takes the atlas updates, since they
    /// won't come again.
    ///
    /// # Errors
    ///
    /// [`Error::Lost`] when the surface is gone and needs a new [`Gpu`], and
    /// [`Error::TooManyShapes`] when the canvas can't fit in its buffers.
    pub fn draw<'a>(
        &mut self,
        canvas: &Canvas,
        atlas: impl IntoIterator<Item = AtlasUpdate<'a>>,
        before_present: impl FnOnce(),
    ) -> Result<(), Error> {
        for update in atlas {
            self.atlas.write(&self.device, &self.queue, &update);
        }
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
        self.upload(canvas)?;
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
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.atlas.bind_group, &[]);
            let mut bound = None;
            for (clip, kind, range) in canvas.batches() {
                let Some([left, top, width, height]) = self.scissor(clip) else {
                    continue;
                };
                if bound != Some(kind) {
                    self.bind(&mut pass, kind);
                    bound = Some(kind);
                }
                pass.set_scissor_rect(left, top, width, height);
                match kind {
                    Kind::Quads => pass.draw(0..4, range_u32(range)?),
                    Kind::Triangles => pass.draw_indexed(range_u32(range)?, 0, 0..1),
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        before_present();
        self.queue.present(frame);
        if suboptimal {
            self.surface.configure(&self.device, &self.config);
        }
        Ok(())
    }

    fn write_viewport(&self) {
        let [width, height] =
            [self.config.width, self.config.height].map(|n| narrow(f64::from(n) / self.scale));
        let viewport = [width, height, narrow(self.scale), 0.];
        self.queue
            .write_buffer(&self.viewport, 0, bytemuck::cast_slice(&viewport));
    }

    fn upload(&mut self, canvas: &Canvas) -> Result<(), Error> {
        let (device, queue) = (&self.device, &self.queue);
        write(device, queue, &mut self.instances, "quads", canvas.quads())?;
        write(
            device,
            queue,
            &mut self.vertices,
            "vertices",
            canvas.vertices(),
        )?;
        write(
            device,
            queue,
            &mut self.indices,
            "indices",
            canvas.indices(),
        )
    }

    fn bind(&self, pass: &mut wgpu::RenderPass, kind: Kind) {
        match kind {
            Kind::Quads => {
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_vertex_buffer(0, self.instances.slice(..));
            }
            Kind::Triangles => {
                pass.set_pipeline(&self.triangle_pipeline);
                pass.set_vertex_buffer(0, self.vertices.slice(..));
                pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
            }
        }
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

fn buffer(
    device: &wgpu::Device,
    label: &'static str,
    usage: wgpu::BufferUsages,
    size: wgpu::BufferAddress,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Writes `items` from the start of `buffer`, first replacing it with one at
/// the next power of two when they don't fit.
fn write<T: bytemuck::Pod>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &mut wgpu::Buffer,
    label: &'static str,
    items: &[T],
) -> Result<(), Error> {
    if items.is_empty() {
        return Ok(());
    }
    let bytes: &[u8] = bytemuck::cast_slice(items);
    let size = wgpu::BufferAddress::try_from(bytes.len()).map_err(|_| Error::TooManyShapes)?;
    if size > buffer.size() {
        let max = device.limits().max_buffer_size;
        if size > max {
            return Err(Error::TooManyShapes);
        }
        let size = size.next_power_of_two().min(max);
        *buffer = self::buffer(device, label, buffer.usage(), size);
    }
    queue.write_buffer(buffer, 0, bytes);
    Ok(())
}

fn range_u32(range: Range<usize>) -> Result<Range<u32>, Error> {
    let start = u32::try_from(range.start).map_err(|_| Error::TooManyShapes)?;
    let end = u32::try_from(range.end).map_err(|_| Error::TooManyShapes)?;
    Ok(start..end)
}

fn quad_pipeline(
    device: &wgpu::Device,
    layouts: [&wgpu::BindGroupLayout; 2],
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("quad.wgsl"));
    let quads = wgpu::VertexBufferLayout {
        array_stride: QUAD_SIZE,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4,
            4 => Float32x4, 5 => Float32, 6 => Uint16x2,
        ],
    };
    pipeline(
        device,
        ("quad", &shader, format),
        &layouts,
        quads,
        wgpu::PrimitiveTopology::TriangleStrip,
    )
}

fn triangle_pipeline(
    device: &wgpu::Device,
    viewport_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("triangle.wgsl"));
    let vertices = wgpu::VertexBufferLayout {
        array_stride: VERTEX_SIZE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
    };
    pipeline(
        device,
        ("triangle", &shader, format),
        &[viewport_layout],
        vertices,
        wgpu::PrimitiveTopology::TriangleList,
    )
}

/// Reads one vertex buffer and blends premultiplied colour over the surface.
fn pipeline(
    device: &wgpu::Device,
    (label, shader, format): (&str, &wgpu::ShaderModule, wgpu::TextureFormat),
    layouts: &[&wgpu::BindGroupLayout],
    buffer: wgpu::VertexBufferLayout,
    topology: wgpu::PrimitiveTopology,
) -> wgpu::RenderPipeline {
    let bind_group_layouts: Vec<_> = layouts.iter().copied().map(Some).collect();
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &bind_group_layouts,
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(buffer)],
        },
        primitive: wgpu::PrimitiveState {
            topology,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
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

impl Atlas {
    /// One 1 by 1 page each until the first update.
    fn new(device: &wgpu::Device) -> Self {
        let entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atlas"),
            entries: &[entry(0), entry(1)],
        });
        let textures =
            [Format::Coverage, Format::Color].map(|format| atlas_texture(device, format, 1, 1));
        let bind_group = atlas_bind_group(device, &layout, &textures);
        Self {
            layout,
            textures,
            bind_group,
        }
    }

    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, update: &AtlasUpdate) {
        let size = u32::from(update.size);
        let pages = u32::from(update.pages);
        let at = usize::from(update.format as u16);
        if self.textures[at].width() != size || self.textures[at].depth_or_array_layers() != pages {
            self.textures[at] = atlas_texture(device, update.format, size, pages);
            self.bind_group = atlas_bind_group(device, &self.layout, &self.textures);
        }
        let texture = &self.textures[at];
        let bytes_per_row = u32::try_from(usize::from(update.size) * update.format.bytes()).ok();
        for write in &update.writes {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: u32::from(write.rows.start),
                        z: u32::from(write.page),
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                write.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row,
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: size,
                    height: u32::from(write.rows.end - write.rows.start),
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

fn atlas_texture(device: &wgpu::Device, format: Format, size: u32, pages: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("atlas"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: pages,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // Colour reads back linear, as the fill and the blending are.
        format: match format {
            Format::Coverage => wgpu::TextureFormat::R8Unorm,
            Format::Color => wgpu::TextureFormat::Rgba8UnormSrgb,
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn atlas_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    textures: &[wgpu::Texture; 2],
) -> wgpu::BindGroup {
    // Said outright, since a texture with one layer views as 2D by default.
    let views = textures.each_ref().map(|texture| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        })
    });
    let [coverage, color] = &views;
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("atlas"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(coverage),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(color),
            },
        ],
    })
}
