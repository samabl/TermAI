//! Offscreen render target, cell-quad pipeline and deterministic readback (kernel/03 stage S8).
//!
//! This module is the first real pixel path of `termai-gpu`. It owns an offscreen
//! `texture + view`, a minimal `wgpu` pipeline that draws cell-aligned quads from a plain
//! descriptor (a rect plus a colour per cell), and a readback that turns the rendered target into
//! a [`PixelBuffer`]. There is **no window, no surface, no swapchain and no present**: kernel/03
//! stage S9 (present) and the ADR-0014 T0-T3 *presentation* ladder are out of scope here, which is
//! what makes the whole path testable headlessly.
//!
//! Glyphs are deliberately absent. ADR-0027 D2 scopes this slice to the GPU draw/commit stage and
//! the crate may not depend on `termai-render` (shaping, S6) or `termai-vt` (S7 consumes S6
//! output), so a "cell" here is a rectangle and a colour. Glyph bitmap integration, the atlas and
//! the real `glyph_bitmap_origin` of RP-05 arrive with the shaping/atlas slice.
//!
//! Grayscale coverage, not blended geometry (AR-14)
//! -----------------------------------------------
//! The quad is rasterised as real triangles (two per cell, dilated by one pixel so that no pixel
//! with non-zero coverage can be missed by the fill rule), and the fragment stage converts the
//! *ideal* float rect into exact per-pixel grayscale coverage. That is AR-14's "grayscale AA,
//! hinting off, strict grid alignment" expressed in the one place a headless test can verify it: it
//! keeps sub-pixel edge placement observable in an 8-bit target without depending on MSAA sample
//! patterns. The coverage function is mirrored on the CPU in [`crate::align::rect_coverage`], and
//! the GPU test compares the two pixel by pixel.
//!
//! Non-gating boundary (ADR-0014)
//! ------------------------------
//! Every number produced through this module is **NON-GATING** unless it came from a T0 backend
//! (DX12 on Windows, Metal on macOS, Vulkan 1.3 on Linux) on an RM-A / RM-C with the pinned
//! reference driver. ADR-0014 rule 4 forbids recording a failing test as a pass on T1/T2/T3, so the
//! tests here assert the measured value on whatever tier the host provides and print the tier they
//! ran on; the authority for what the number means is kernel/03 RP-05, not this crate.

use core::fmt;
use std::error::Error;

use crate::align::{
    measure_alignment, AlignmentPattern, GridAlignment, MeasureError, PixelBuffer, Rect,
};
use crate::probe::{block_on, select_adapter, ProbeReport, ProbeUnavailable};

/// Texel format of the offscreen target: 8-bit RGBA, renderable and copyable everywhere.
pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Vertex stride in bytes: position (2 floats) + rect (4) + colour (4).
const VERTEX_STRIDE: u64 = 40;

/// Two triangles per cell quad.
const VERTICES_PER_QUAD: u32 = 6;

/// How far a quad is grown for rasterisation, in pixels, on top of the exact coverage test.
///
/// A pixel whose centre is outside the true rect but whose square overlaps it must still reach the
/// fragment stage, or its partial coverage would be lost; one pixel of dilation guarantees that for
/// axis-aligned rects because any overlapping pixel centre is at most half a pixel outside.
const QUAD_DILATION_PX: f32 = 1.0;

/// The WGSL for the cell-quad pipeline.
///
/// `axis_coverage` must stay identical in intent to [`crate::align::rect_coverage`]; the GPU test
/// `offscreen_readback_matches_the_cpu_coverage_mirror` fails if the two drift apart.
const QUAD_SHADER: &str = r"
struct Uniforms {
    size: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) position_px: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) color: vec4<f32>,
) -> VertexOutput {
    var output: VertexOutput;
    let ndc = vec2<f32>(
        position_px.x / uniforms.size.x * 2.0 - 1.0,
        1.0 - position_px.y / uniforms.size.y * 2.0,
    );
    output.position = vec4<f32>(ndc, 0.0, 1.0);
    output.rect = rect;
    output.color = color;
    return output;
}

fn axis_coverage(centre: f32, lo: f32, hi: f32) -> f32 {
    let upper = clamp(centre + 0.5, lo, hi);
    let lower = clamp(centre - 0.5, lo, hi);
    return max(upper - lower, 0.0);
}

@fragment
fn fs_main(fragment: VertexOutput) -> @location(0) vec4<f32> {
    let pixel = fragment.position.xy;
    let rect = fragment.rect;
    let coverage = axis_coverage(pixel.x, rect.x, rect.x + rect.z)
        * axis_coverage(pixel.y, rect.y, rect.y + rect.w);
    return vec4<f32>(fragment.color.rgb * coverage, fragment.color.a * coverage);
}
";

/// One cell-aligned quad: the rect to draw, in physical pixels, and its RGBA colour.
///
/// This is the "plain descriptor" of this slice. A later slice replaces the colour with an atlas
/// slot and an advance; the rect stays the grid-authoritative cell box.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CellQuad {
    /// Quad rect in physical pixels, origin at the top-left of the target.
    pub rect: Rect,
    /// RGBA colour, 0.0..=1.0 per channel.
    pub color: [f32; 4],
}

/// Why an offscreen render could not be produced.
///
/// None of these paths panics: a missing adapter, an unavailable wgpu, a rejected device or a
/// failed mapping all come back as a value (AGENTS section 6).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GpuError {
    /// wgpu could not be started at all (no backend feature compiled in, or enumeration stalled).
    ProbeUnavailable(ProbeUnavailable),
    /// wgpu started and enumerated no adapter. The CI case, and the T3 safe-mode case.
    NoAdapter,
    /// The adapter was found but its device request never became ready.
    AdapterNotReady,
    /// The adapter rejected the device request; the driver message is carried verbatim.
    DeviceRequest(String),
    /// A poll of the device failed.
    Poll(String),
    /// The rendered target could not be mapped back into host memory.
    Readback(String),
    /// The requested target is larger than the device allows.
    TargetTooLarge {
        /// Requested `(width, height)`.
        requested: (u32, u32),
        /// The device's `max_texture_dimension_2d`.
        limit: u32,
    },
    /// A zero-sized offscreen target.
    EmptyTarget,
    /// The readback pixels could not be measured (see [`MeasureError`]).
    Measurement(MeasureError),
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProbeUnavailable(reason) => match reason {
                ProbeUnavailable::NoBackendFeature => f.write_str(
                    "wgpu has no backend feature compiled in for this target, so the probe cannot run",
                ),
                ProbeUnavailable::EnumerationNotReady => {
                    f.write_str("wgpu adapter enumeration never became ready")
                }
            },
            Self::NoAdapter => f.write_str("wgpu enumerated no adapter on this host"),
            Self::AdapterNotReady => f.write_str("the adapter request never became ready"),
            Self::DeviceRequest(message) => write!(f, "the adapter refused a device: {message}"),
            Self::Poll(message) => write!(f, "polling the device failed: {message}"),
            Self::Readback(message) => write!(f, "reading the rendered target back failed: {message}"),
            Self::TargetTooLarge { requested, limit } => write!(
                f,
                "the offscreen target {}x{} exceeds the device limit of {limit}",
                requested.0, requested.1
            ),
            Self::EmptyTarget => f.write_str("an offscreen target must have a non-zero size"),
            Self::Measurement(error) => write!(f, "the readback could not be measured: {error}"),
        }
    }
}

impl Error for GpuError {}

/// One vertex of the cell-quad pipeline, in physical pixels.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct Vertex {
    position: [f32; 2],
    rect: [f32; 4],
    color: [f32; 4],
}

/// The vertex-stage uniform block: the target size in physical pixels.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct Uniforms {
    size: [f32; 4],
}

/// An offscreen render target plus the pipeline that draws into it.
///
/// The device is acquired through [`crate::probe::select_adapter`], so adapter selection and the
/// ADR-0014 T0-T3 classification exist exactly once in this crate; [`OffscreenRenderer::probe_report`]
/// exposes what that selection concluded about this host.
pub struct OffscreenRenderer {
    _instance: Option<wgpu::Instance>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    vertex_quads: usize,
    uniforms: wgpu::Buffer,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    readback_bytes_per_row: u32,
    width: u32,
    height: u32,
    report: ProbeReport,
}

impl OffscreenRenderer {
    /// Acquire an adapter, a device and an offscreen target of `width x height` physical pixels.
    ///
    /// # Errors
    ///
    /// [`GpuError::ProbeUnavailable`] or [`GpuError::NoAdapter`] when this host cannot provide a
    /// device (the honest-unavailable case), [`GpuError::DeviceRequest`] when the adapter rejects
    /// the request, and [`GpuError::EmptyTarget`] / [`GpuError::TargetTooLarge`] for bad sizes.
    pub fn new(width: u32, height: u32) -> Result<Self, GpuError> {
        let selection = select_adapter();
        let report = selection.report;
        let Some(adapter) = selection.adapter else {
            return Err(match report.unavailable {
                Some(reason) => GpuError::ProbeUnavailable(reason),
                None => GpuError::NoAdapter,
            });
        };

        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("termai-gpu.offscreen.device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        }))
        .ok_or(GpuError::AdapterNotReady)?
        .map_err(|error| GpuError::DeviceRequest(error.to_string()))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("termai-gpu.offscreen.quads"),
            source: wgpu::ShaderSource::Wgsl(QUAD_SHADER.into()),
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("termai-gpu.offscreen.uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("termai-gpu.offscreen.layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("termai-gpu.offscreen.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX_STRIDE,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 8,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 24,
                            shader_location: 2,
                        },
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TARGET_FORMAT,
                    // Grayscale coverage is composited source-over, not replaced. Because a quad is
                    // rasterised one pixel larger than its true rect, a neighbouring quad's fragment
                    // can land on a pixel it does not cover; with `blend: None` that zero-coverage
                    // fragment would erase the pixel a previous quad already wrote, and the readback
                    // would show holes at cell edges (the GPU/CPU mirror test catches exactly that).
                    // The fragment already outputs premultiplied colour, so the premultiplied
                    // source-over state is the right one (AR-14 grayscale AA, no subpixel).
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("termai-gpu.offscreen.uniforms"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("termai-gpu.offscreen.bind_group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });

        let vertices = create_vertex_buffer(&device, 1);
        let (target, view) = create_target(&device, 1, 1);
        let readback = create_readback_buffer(&device, 1, 1);
        let mut renderer = Self {
            _instance: selection.instance,
            device,
            queue,
            pipeline,
            bind_group,
            vertices,
            vertex_quads: 1,
            uniforms,
            target,
            view,
            readback,
            readback_bytes_per_row: align_up(4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT),
            width: 1,
            height: 1,
            report,
        };
        renderer.resize(width, height)?;
        Ok(renderer)
    }

    /// What the ADR-0014 probe concluded about the host this renderer runs on.
    #[must_use]
    pub fn probe_report(&self) -> &ProbeReport {
        &self.report
    }

    /// The offscreen target size in physical pixels.
    #[must_use]
    pub const fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The offscreen texture, for a caller that wants to sample or inspect it directly.
    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.target
    }

    /// The texture view used as the colour attachment.
    #[must_use]
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Re-create the offscreen target and its readback buffer at a new size.
    ///
    /// The device, queue and pipeline are reused, so a caller that measures several DPI scales pays
    /// for one device acquisition.
    ///
    /// # Errors
    ///
    /// [`GpuError::EmptyTarget`] for a zero size and [`GpuError::TargetTooLarge`] above the
    /// device's `max_texture_dimension_2d`.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), GpuError> {
        if width == 0 || height == 0 {
            return Err(GpuError::EmptyTarget);
        }
        let limit = self.device.limits().max_texture_dimension_2d;
        if width > limit || height > limit {
            return Err(GpuError::TargetTooLarge {
                requested: (width, height),
                limit,
            });
        }
        if width == self.width && height == self.height {
            return Ok(());
        }

        let (target, view) = create_target(&self.device, width, height);
        self.target = target;
        self.view = view;
        self.readback = create_readback_buffer(&self.device, width, height);
        self.readback_bytes_per_row = align_up(width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        self.width = width;
        self.height = height;
        Ok(())
    }

    /// Draw `cells` into the offscreen target; nothing is presented anywhere.
    ///
    /// Nothing here is fallible in practice: the result is uniform with [`OffscreenRenderer::render`]
    /// so a caller can keep one `?` path, and the vertex buffer grows on demand.
    ///
    /// # Errors
    ///
    /// Reserved for driver-side failures surfaced by a later slice; no input accepted by
    /// [`OffscreenRenderer::new`] currently makes this return `Err`.
    pub fn draw(&mut self, cells: &[CellQuad]) -> Result<(), GpuError> {
        if cells.len() > self.vertex_quads {
            self.vertex_quads = cells.len().next_power_of_two();
            self.vertices = create_vertex_buffer(&self.device, self.vertex_quads);
        }

        let mut vertices = Vec::with_capacity(cells.len() * VERTICES_PER_QUAD as usize);
        for cell in cells {
            push_quad(&mut vertices, cell);
        }

        let uniforms = Uniforms {
            size: [self.width as f32, self.height as f32, 0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniforms, 0, &float_bytes(&uniforms.size));
        if !vertices.is_empty() {
            self.queue
                .write_buffer(&self.vertices, 0, &vertex_bytes(&vertices));
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("termai-gpu.offscreen.draw"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("termai-gpu.offscreen.pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Transparent black: with premultiplied source-over, one quad's coverage
                        // lands in the target's red channel unchanged.
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Copy the offscreen target back to host memory as a compact RGBA8 buffer.
    ///
    /// # Errors
    ///
    /// [`GpuError::Poll`] when the device cannot be polled and [`GpuError::Readback`] when the
    /// mapping fails or the mapped bytes do not match the target size.
    pub fn read_back(&self) -> Result<PixelBuffer, GpuError> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("termai-gpu.offscreen.readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.readback_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = self.readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| GpuError::Poll(error.to_string()))?;
        match receiver.recv() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(GpuError::Readback(error.to_string())),
            Err(_) => {
                return Err(GpuError::Readback(
                    "the map callback was dropped".to_string(),
                ))
            }
        }

        let compacted = {
            let view = slice
                .get_mapped_range()
                .map_err(|error| GpuError::Readback(error.to_string()))?;
            compact_rows(&view, self.width, self.height, self.readback_bytes_per_row)
        };
        self.readback.unmap();

        PixelBuffer::from_rgba8(self.width, self.height, compacted)
            .map_err(|error| GpuError::Readback(error.to_string()))
    }

    /// Draw `cells`, read the target back and return the pixels. Deterministic for fixed input.
    ///
    /// # Errors
    ///
    /// Whatever [`OffscreenRenderer::draw`] or [`OffscreenRenderer::read_back`] returns.
    pub fn render(&mut self, cells: &[CellQuad]) -> Result<PixelBuffer, GpuError> {
        self.draw(cells)?;
        self.read_back()
    }

    /// Render the alignment pattern at its own size, read it back and measure it (kernel/03 RP-05).
    ///
    /// The returned [`GridAlignment`] carries the worst deviation **and** the edge sample count
    /// behind it. Remember ADR-0014: on a non-T0 backend that number is NON-GATING.
    ///
    /// # Errors
    ///
    /// The GPU errors above, plus [`GpuError::Measurement`] when the readback cannot be measured.
    pub fn measure(&mut self, pattern: &AlignmentPattern) -> Result<GridAlignment, GpuError> {
        self.resize(pattern.width, pattern.height)?;
        let quads = cell_quads(pattern);
        let pixels = self.render(&quads)?;
        measure_alignment(&pixels, pattern).map_err(GpuError::Measurement)
    }
}

/// The [`CellQuad`] list for an alignment pattern: the *drawn* rect of every pattern cell.
#[must_use]
pub fn cell_quads(pattern: &AlignmentPattern) -> Vec<CellQuad> {
    pattern
        .cells
        .iter()
        .map(|cell| CellQuad {
            rect: cell.drawn,
            color: cell.color,
        })
        .collect()
}

/// Append the six vertices (two triangles) of one dilated cell quad.
fn push_quad(vertices: &mut Vec<Vertex>, cell: &CellQuad) {
    let area = cell.rect.expanded(QUAD_DILATION_PX);
    let rect = [cell.rect.x, cell.rect.y, cell.rect.w, cell.rect.h];
    let corners = [
        (area.x, area.y),
        (area.right(), area.y),
        (area.x, area.bottom()),
        (area.right(), area.y),
        (area.right(), area.bottom()),
        (area.x, area.bottom()),
    ];
    for (x, y) in corners {
        vertices.push(Vertex {
            position: [x, y],
            rect,
            color: cell.color,
        });
    }
}

/// Serialise vertices into the little-endian float layout the vertex buffer expects.
fn vertex_bytes(vertices: &[Vertex]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vertices.len() * VERTEX_STRIDE as usize);
    for vertex in vertices {
        let values = vertex
            .position
            .iter()
            .chain(vertex.rect.iter())
            .chain(vertex.color.iter());
        for value in values {
            bytes.extend_from_slice(&value.to_ne_bytes());
        }
    }
    bytes
}

/// Serialise a float slice as raw native-endian bytes (the same layout `#[repr(C)]` describes).
fn float_bytes(values: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }
    bytes
}

/// Round `value` up to the next multiple of `align` (`u32::div_ceil` is stable since 1.73).
const fn align_up(value: u32, align: u32) -> u32 {
    if align == 0 {
        return value;
    }
    value.div_ceil(align) * align
}

/// Create the offscreen colour target and its view.
fn create_target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("termai-gpu.offscreen.target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Create a readback buffer big enough for the padded rows of a `width x height` target.
fn create_readback_buffer(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Buffer {
    let bytes_per_row = align_up(width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("termai-gpu.offscreen.readback"),
        size: u64::from(bytes_per_row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

/// Create the vertex buffer for up to `quads` cell quads.
fn create_vertex_buffer(device: &wgpu::Device, quads: usize) -> wgpu::Buffer {
    let size = (quads.max(1) as u64) * u64::from(VERTICES_PER_QUAD) * VERTEX_STRIDE;
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("termai-gpu.offscreen.vertices"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Drop the row padding a `copy_texture_to_buffer` readback necessarily carries.
fn compact_rows(view: &[u8], width: u32, height: u32, bytes_per_row: u32) -> Vec<u8> {
    let row_bytes = width as usize * 4;
    let mut out = Vec::with_capacity(row_bytes * height as usize);
    for row in 0..height as usize {
        let start = row * bytes_per_row as usize;
        if let Some(chunk) = view.get(start..start + row_bytes) {
            out.extend_from_slice(chunk);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::align::{rasterize_cpu, GridGeometry, RP05_DPI_SCALES};

    /// JetBrains Mono class metrics: 13px font, 0.6 advance ratio, AR-22's 1.25 line height.
    const ADVANCE_PX: f32 = 7.8;
    const FONT_SIZE_PX: f32 = 13.0;
    const COLS: u16 = 6;
    const ROWS: u16 = 4;

    fn geometry(scale: f32) -> GridGeometry {
        GridGeometry::from_metrics(COLS, ROWS, ADVANCE_PX, FONT_SIZE_PX, scale)
    }

    /// One-line summary of the ADR-0014 probe outcome, including the tier this host lands on.
    fn probe_line(report: &ProbeReport) -> String {
        let classification = report.classification;
        let adapter = match &report.adapter {
            Some(adapter) => format!(
                "{} [{} {} driver={} {}]",
                adapter.name,
                adapter.api.label(),
                match adapter.class {
                    crate::AdapterClass::Hardware => "hardware",
                    crate::AdapterClass::Software => "software",
                    crate::AdapterClass::Unclassified => "unclassified",
                },
                adapter.driver,
                adapter.driver_info
            ),
            None => "none".to_string(),
        };
        format!(
            "level={:?} reason={:?} gate_eligible={} adapters={} unavailable={:?} adapter={}",
            classification.level,
            classification.reason,
            classification.gate_eligible,
            report.adapter_count,
            report.unavailable,
            adapter
        )
    }

    /// Acquire an offscreen renderer, or report honestly that this host has no adapter.
    ///
    /// ADR-0014 rule 4: an unavailable backend is not a pass, so this prints the reason and returns
    /// None instead of reporting a green measurement.
    fn renderer_or_unavailable(pattern: &AlignmentPattern) -> Option<OffscreenRenderer> {
        match OffscreenRenderer::new(pattern.width, pattern.height) {
            Ok(renderer) => Some(renderer),
            Err(error @ (GpuError::ProbeUnavailable(_) | GpuError::NoAdapter)) => {
                println!(
                    "termai-gpu offscreen renderer UNAVAILABLE on this host: {error} \
                     (this GPU test did NOT run; NON-GATING per ADR-0014)"
                );
                None
            }
            Err(error) => panic!("the offscreen renderer could not be created: {error}"),
        }
    }

    #[test]
    fn alignment_measurement_stays_within_the_rp05_contract() {
        let probed = crate::probe::probe();
        println!("adapter probe (raw): {probed:#?}");
        println!("adapter probe (line): {}", probe_line(&probed));

        let first = AlignmentPattern::checkerboard(geometry(RP05_DPI_SCALES[0]));
        let Some(mut renderer) = renderer_or_unavailable(&first) else {
            return;
        };

        let report = renderer.probe_report().clone();
        let tier = report.classification.level;
        println!(
            "offscreen tier={tier:?} gate_eligible={} -> this number is {}",
            report.classification.gate_eligible,
            if report.classification.gate_eligible {
                "a T0 gate-eligible result"
            } else {
                "NON-GATING (ADR-0014: only RM-A/RM-C T0 with the pinned driver may judge section 5)"
            }
        );

        for scale in RP05_DPI_SCALES {
            let pattern = AlignmentPattern::checkerboard(geometry(scale));
            let measured = match renderer.measure(&pattern) {
                Ok(measured) => measured,
                Err(error) => {
                    panic!("the GPU measurement failed after the device was acquired: {error}")
                }
            };
            println!("measured: {measured}");

            assert_eq!(
                measured.samples,
                pattern.cells.len() * 4,
                "every drawn cell contributes four edge samples"
            );
            assert!(
                measured.worst_px <= crate::align::HARNESS_ALIGNMENT_CONTRACT_PX,
                "kernel/03 RP-05 / HARNESS section 5: {measured}"
            );
        }

        // Self-check: an injected 0.75px misalignment must be caught, so the mechanism above is
        // demonstrably able to fail rather than always reporting green.
        let shifted = AlignmentPattern::shifted(geometry(1.25), 0.75, -0.6);
        let measured = match renderer.measure(&shifted) {
            Ok(measured) => measured,
            Err(error) => panic!("the shifted-pattern measurement failed: {error}"),
        };
        println!("injected 0.75px/-0.60px -> {measured}");
        assert!(
            measured.worst_px > crate::align::HARNESS_ALIGNMENT_CONTRACT_PX,
            "the GPU mechanism must catch an injected misalignment: {measured}"
        );
        assert!(measured.worst_px < 1.0, "{measured}");
    }

    #[test]
    fn offscreen_readback_matches_the_cpu_coverage_mirror() {
        let pattern = AlignmentPattern::checkerboard(geometry(1.25));
        let Some(mut renderer) = renderer_or_unavailable(&pattern) else {
            return;
        };

        let quads = cell_quads(&pattern);
        let first = match renderer.render(&quads) {
            Ok(pixels) => pixels,
            Err(error) => panic!("the offscreen render failed: {error}"),
        };
        let second = match renderer.render(&quads) {
            Ok(pixels) => pixels,
            Err(error) => panic!("the repeated offscreen render failed: {error}"),
        };
        assert_eq!(
            first.data(),
            second.data(),
            "readback must be deterministic"
        );

        let mirror = rasterize_cpu(&pattern).expect("the CPU rasteriser sizes its own buffer");
        assert_eq!(
            (mirror.width(), mirror.height()),
            (first.width(), first.height())
        );
        let mut worst = 0i32;
        let mut worst_at = (0u32, 0u32, 0i32, 0i32);
        let mut differing = 0usize;
        for y in 0..first.height() {
            for x in 0..first.width() {
                let index = (y as usize * first.width() as usize + x as usize) * 4;
                let gpu = i32::from(first.data()[index]);
                let cpu = i32::from(mirror.data()[index]);
                let diff = (gpu - cpu).abs();
                if diff != 0 {
                    differing += 1;
                }
                if diff > worst {
                    worst = diff;
                    worst_at = (x, y, gpu, cpu);
                }
            }
        }
        println!(
            "GPU vs CPU coverage mirror: {}x{} px, worst channel difference {worst}/255 at (x, y, gpu, cpu)={worst_at:?}, {differing} differing pixel(s)",
            first.width(),
            first.height()
        );
        // Two codes of slack: the fixed-point conversion may round exact .5 ties either way.
        assert!(
            worst <= 2,
            "the WGSL coverage function and its CPU mirror have drifted apart: worst difference {worst}/255 at {worst_at:?}"
        );
    }
}
