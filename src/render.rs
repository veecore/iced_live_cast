//! Custom renderer for live cast frames.

use crate::frame::Frame;
use bytemuck::{Pod, Zeroable};
use iced::Rectangle;
use iced_wgpu::primitive::{Pipeline, Primitive};
use iced_wgpu::wgpu;
use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Tiny WGSL program used to draw one textured quad.
const SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct Uniforms {
    rect: vec4<f32>,
}

@group(0) @binding(0)
var live_texture: texture_2d<f32>;

@group(0) @binding(1)
var live_sampler: sampler;

@group(0) @binding(2)
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var quad = array<vec4<f32>, 6>(
        vec4<f32>(uniforms.rect.xy, 0.0, 0.0),
        vec4<f32>(uniforms.rect.zy, 1.0, 0.0),
        vec4<f32>(uniforms.rect.xw, 0.0, 1.0),
        vec4<f32>(uniforms.rect.zy, 1.0, 0.0),
        vec4<f32>(uniforms.rect.zw, 1.0, 1.0),
        vec4<f32>(uniforms.rect.xw, 0.0, 1.0),
    );

    var out: VertexOutput;
    out.uv = quad[in_vertex_index].zw;
    out.position = vec4<f32>(quad[in_vertex_index].xy, 1.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(live_texture, live_sampler, in.uv);
    return vec4<f32>(sampled.rgb, 1.0);
}
"#;

/// Uniform block passed to the textured-quad shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    /// Rectangle in clip space: left, top, right, bottom.
    rect: [f32; 4],
    /// Padding required by the uniform-buffer alignment rules.
    _padding: [u8; 240],
}

/// GPU resources cached for one live cast handle.
struct Entry {
    /// Shared liveness flag used to evict dead handles from the cache.
    alive: Arc<AtomicBool>,
    /// Texture storing the latest uploaded frame.
    texture: wgpu::Texture,
    /// Uniform buffer storing draw rectangles for the current frame.
    uniforms: wgpu::Buffer,
    /// Bind group referencing the texture, sampler, and uniforms.
    bind_group: wgpu::BindGroup,
    /// Last frame generation uploaded into the texture.
    last_uploaded_generation: AtomicU64,
    /// Number of times the entry was prepared in the current render frame.
    prepare_index: AtomicUsize,
    /// Number of times the entry was rendered in the current render frame.
    render_index: AtomicUsize,
    /// Width of the current texture in pixels.
    width: u32,
    /// Height of the current texture in pixels.
    height: u32,
}

/// Shared renderer state for all cast primitives.
pub(crate) struct CastViewPipeline {
    /// Render pipeline used to draw textured quads.
    pipeline: wgpu::RenderPipeline,
    /// Bind-group layout shared by all live textures.
    bind_group_layout: wgpu::BindGroupLayout,
    /// Sampler used by all live textures.
    sampler: wgpu::Sampler,
    /// Cache keyed by handle identifier.
    entries: BTreeMap<u64, Entry>,
}

impl Pipeline for CastViewPipeline {
    /// Builds the pipeline once for the current target format.
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("iced_live_cast shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("iced_live_cast bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<Uniforms>() as u64),
                    },
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("iced_live_cast pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("iced_live_cast pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("iced_live_cast sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 1.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            entries: BTreeMap::new(),
        }
    }

    /// Drops cache entries once their handles have died.
    fn trim(&mut self) {
        self.entries.retain(|_, entry| {
            if entry.alive.load(Ordering::SeqCst) {
                true
            } else {
                entry.texture.destroy();
                entry.uniforms.destroy();
                false
            }
        });
    }
}

impl CastViewPipeline {
    /// Uploads one frame into the cache entry for the given handle.
    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        primitive: &CastViewPrimitive,
        frame: &Frame,
    ) {
        // We reuse one cache entry per handle and only rebuild GPU resources when
        // the frame geometry changes. Ordinary frame delivery only overwrites the
        // same texture, so the pipeline itself is not recreated every redraw.
        let needs_recreate = needs_entry_recreate(
            self.entries
                .get(&primitive.handle_id)
                .map(|entry| (entry.width, entry.height)),
            frame,
        );

        if needs_recreate {
            self.entries.remove(&primitive.handle_id);
            self.entries.insert(
                primitive.handle_id,
                create_entry(
                    device,
                    &self.bind_group_layout,
                    &self.sampler,
                    primitive,
                    frame,
                ),
            );
        }

        let Some(entry) = self.entries.get(&primitive.handle_id) else {
            return;
        };

        // Sources bump generations when they replace the current frame, so we do
        // not hash frame bytes here. Live display almost always changes, and the
        // generation gate is enough to skip duplicate uploads from repeated draws.
        if entry.last_uploaded_generation.load(Ordering::Relaxed) == primitive.generation {
            return;
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &entry.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            frame.pixels(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.bytes_per_row()),
                rows_per_image: Some(frame.height()),
            },
            wgpu::Extent3d {
                width: frame.width(),
                height: frame.height(),
                depth_or_array_layers: 1,
            },
        );

        entry
            .last_uploaded_generation
            .store(primitive.generation, Ordering::Relaxed);
    }

    /// Writes the transformed bounds into the per-entry uniform buffer.
    fn prepare(&mut self, queue: &wgpu::Queue, handle_id: u64, bounds: &Rectangle) {
        let Some(entry) = self.entries.get_mut(&handle_id) else {
            return;
        };

        let uniforms = Uniforms {
            rect: [
                bounds.x,
                bounds.y,
                bounds.x + bounds.width,
                bounds.y + bounds.height,
            ],
            _padding: [0; 240],
        };

        queue.write_buffer(
            &entry.uniforms,
            (entry.prepare_index.load(Ordering::Relaxed) * std::mem::size_of::<Uniforms>()) as u64,
            bytemuck::bytes_of(&uniforms),
        );
        entry.prepare_index.fetch_add(1, Ordering::Relaxed);
        entry.render_index.store(0, Ordering::Relaxed);
    }

    /// Draws one prepared entry into the current render target.
    fn draw(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        clip: &Rectangle<u32>,
        handle_id: u64,
    ) {
        let Some(entry) = self.entries.get(&handle_id) else {
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("iced_live_cast render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(
            0,
            &entry.bind_group,
            &[
                (entry.render_index.load(Ordering::Relaxed) * std::mem::size_of::<Uniforms>())
                    as u32,
            ],
        );
        pass.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
        pass.draw(0..6, 0..1);

        entry.prepare_index.store(0, Ordering::Relaxed);
        entry.render_index.fetch_add(1, Ordering::Relaxed);
    }
}

/// Primitive drawn by the active cast widget.
#[derive(Debug, Clone)]
pub(crate) struct CastViewPrimitive {
    /// Identifier of the handle being rendered.
    pub handle_id: u64,
    /// Shared liveness flag for cache eviction.
    pub alive: Arc<AtomicBool>,
    /// Latest frame snapshot available for upload.
    pub frame: Option<Frame>,
    /// Frame generation associated with the snapshot.
    pub generation: u64,
}

impl Primitive for CastViewPrimitive {
    type Pipeline = CastViewPipeline;

    /// Uploads the latest frame and prepares the draw uniforms.
    fn prepare(
        &self,
        pipeline: &mut CastViewPipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &iced_wgpu::graphics::Viewport,
    ) {
        if let Some(frame) = self.frame.as_ref() {
            pipeline.upload(device, queue, self, frame);
        }

        pipeline.prepare(
            queue,
            self.handle_id,
            &(*bounds
                * iced::Transformation::orthographic(
                    viewport.logical_size().width as u32,
                    viewport.logical_size().height as u32,
                )),
        );
    }

    /// Renders the prepared primitive into the current target.
    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        pipeline.draw(target, encoder, clip_bounds, self.handle_id);
    }
}

/// Creates one cache entry for the current handle and frame shape.
fn create_entry(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    primitive: &CastViewPrimitive,
    frame: &Frame,
) -> Entry {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("iced_live_cast texture"),
        size: wgpu::Extent3d {
            width: frame.width(),
            height: frame.height(),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: texture_format(),
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("iced_live_cast uniform buffer"),
        size: 256 * std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("iced_live_cast bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniforms,
                    offset: 0,
                    size: NonZeroU64::new(std::mem::size_of::<Uniforms>() as u64),
                }),
            },
        ],
    });

    Entry {
        alive: Arc::clone(&primitive.alive),
        texture,
        uniforms,
        bind_group,
        last_uploaded_generation: AtomicU64::new(u64::MAX),
        prepare_index: AtomicUsize::new(0),
        render_index: AtomicUsize::new(0),
        width: frame.width(),
        height: frame.height(),
    }
}

/// Returns the texture format used by every uploaded live frame.
fn texture_format() -> wgpu::TextureFormat {
    wgpu::TextureFormat::Bgra8Unorm
}

/// Returns whether one cached entry must be rebuilt for the supplied frame geometry.
fn needs_entry_recreate(existing_dimensions: Option<(u32, u32)>, frame: &Frame) -> bool {
    existing_dimensions
        .map(|(width, height)| width != frame.width() || height != frame.height())
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::{needs_entry_recreate, texture_format};
    use crate::Frame;
    use iced_wgpu::wgpu;

    /// Live frames should upload into one non-sRGB BGRA texture format.
    #[test]
    fn live_frames_use_bgra8_unorm() {
        assert_eq!(texture_format(), wgpu::TextureFormat::Bgra8Unorm);
    }

    /// Reused cache entries should survive when the next frame keeps the same geometry.
    #[test]
    fn matching_frame_dimensions_reuse_the_existing_entry() {
        let frame = sample_frame(1280, 720);

        assert!(!needs_entry_recreate(Some((1280, 720)), &frame));
    }

    /// Texture entries should be rebuilt when the frame geometry changes.
    #[test]
    fn changed_frame_dimensions_force_entry_recreation() {
        let frame = sample_frame(1920, 1080);

        assert!(needs_entry_recreate(Some((1280, 720)), &frame));
    }

    /// Builds one sample frame for renderer tests.
    fn sample_frame(width: u32, height: u32) -> Frame {
        Frame::from_bgra(width, height, vec![0; width as usize * height as usize * 4])
            .expect("sample frame should validate")
    }
}
