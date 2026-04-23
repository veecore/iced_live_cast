//! Custom live-raster primitive for [`crate::widget::CastView`].

use crate::frame::Frame;
use crate::handle::{CastHandle, Source};
use bytemuck::{Pod, Zeroable};
use iced::advanced::{self, image as core_image};
use iced::border;
use iced::widget::image::FilterMethod;
use iced::{Point, Rectangle, Rotation, Size};
use iced_wgpu::primitive::{Pipeline, Primitive, Renderer as PrimitiveRenderer};
use iced_wgpu::wgpu;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// A live raster image ready to be drawn by one renderer backend.
#[derive(Debug, Clone)]
pub struct LiveImage {
    /// Stable key identifying the shared live texture for this image stream.
    pub key: u64,
    /// Generation of the current frame inside the keyed live texture.
    pub generation: u64,
    /// Latest frame snapshot to upload before drawing.
    pub frame: Frame,
    /// Sampling filter requested by the wrapped stock image widget.
    pub filter_method: FilterMethod,
    /// Rotation to apply around the image center.
    pub rotation: iced::Radians,
    /// Border radius applied around the image clip bounds.
    pub border_radius: border::Radius,
    /// Opacity multiplier for the final image draw.
    pub opacity: f32,
    /// Whether the image should be snapped to the pixel grid.
    pub snap: bool,
    /// Shared liveness flag used internally for cache eviction.
    pub(crate) alive: Arc<AtomicBool>,
}

impl LiveImage {
    /// Builds one semantic live-image draw request from the stock image payload.
    pub(crate) fn from_draw_request<S: Source>(
        image: core_image::Image<CastHandle<S>>,
    ) -> Option<Self> {
        let frame = image.handle.snapshot()?;

        Some(Self {
            key: image.handle.inner.id,
            generation: image.handle.inner.generation.load(Ordering::Relaxed),
            frame,
            filter_method: image.filter_method,
            rotation: image.rotation,
            border_radius: image.border_radius,
            opacity: image.opacity,
            snap: image.snap,
            alive: Arc::clone(&image.handle.inner.alive),
        })
    }
}

/// Renderer contract used by [`crate::widget::CastView`].
///
/// The widget emits one semantic live image and lets the backend decide how to
/// keep the no-cache mutable-texture path efficient.
pub trait LiveRasterRenderer {
    /// Draws one semantic live image using the same bounds and clip bounds that
    /// the stock image widget would use.
    fn draw_live_image(&mut self, image: LiveImage, bounds: Rectangle, clip_bounds: Rectangle);
}

/// Thin adapter that lets any primitive-capable `iced` renderer satisfy the
/// live-raster renderer contract.
pub(crate) struct PrimitiveLiveRasterRenderer<'a, Renderer> {
    /// Underlying `iced` renderer receiving the final primitive draw call.
    renderer: &'a mut Renderer,
}

impl<'a, Renderer> PrimitiveLiveRasterRenderer<'a, Renderer> {
    /// Wraps one primitive-capable renderer.
    pub(crate) fn new(renderer: &'a mut Renderer) -> Self {
        Self { renderer }
    }
}

/// Live-raster shader modeled after `iced_wgpu`'s stock image renderer, but
/// adapted to sample one mutable `texture_2d` per cast handle instead of one
/// cached atlas layer.
const SHADER: &str = r#"
fn vertex_position(vertex_index: u32) -> vec2<f32> {
    return vec2<f32>(
        (vec2<u32>(1u, 2u) + vertex_index) % vec2<u32>(6u) < vec2<u32>(3u)
    );
}

struct Globals {
    size: vec2<f32>,
    scale_factor: f32,
    _padding: f32,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var u_sampler: sampler;
@group(1) @binding(0) var u_texture: texture_2d<f32>;

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) center: vec2<f32>,
    @location(1) clip_bounds: vec4<f32>,
    @location(2) border_radius: vec4<f32>,
    @location(3) tile: vec4<f32>,
    @location(4) rotation: f32,
    @location(5) opacity: f32,
    @location(6) texture_pos: vec2<f32>,
    @location(7) texture_scale: vec2<f32>,
    @location(8) snap: u32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) clip_bounds: vec4<f32>,
    @location(1) @interpolate(flat) border_radius: vec4<f32>,
    @location(2) @interpolate(flat) texture_bounds: vec4<f32>,
    @location(3) @interpolate(flat) opacity: f32,
    @location(4) uv: vec2<f32>,
    @location(5) local_position: vec2<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let corner = vertex_position(input.vertex_index);
    let tile = input.tile;

    let corners = array<vec2<f32>, 4>(
        tile.xy,
        tile.xy + vec2<f32>(tile.z, 0.0),
        tile.xy + vec2<f32>(0.0, tile.w),
        tile.xy + tile.zw,
    );

    let cos_r = cos(-input.rotation);
    let sin_r = sin(-input.rotation);
    var rotated = array<vec2<f32>, 4>();

    for (var i = 0u; i < 4u; i++) {
        let c = corners[i] - input.center;
        rotated[i] = vec2<f32>(
            c.x * cos_r - c.y * sin_r,
            c.x * sin_r + c.y * cos_r,
        ) + input.center;
    }

    var min_xy = rotated[0];
    var max_xy = rotated[0];

    for (var i = 1u; i < 4u; i++) {
        min_xy = min(min_xy, rotated[i]);
        max_xy = max(max_xy, rotated[i]);
    }

    let rotated_bounds = vec4<f32>(min_xy, max_xy - min_xy);
    let clip_min = max(rotated_bounds.xy, input.clip_bounds.xy);
    let clip_max = min(
        rotated_bounds.xy + rotated_bounds.zw,
        input.clip_bounds.xy + input.clip_bounds.zw,
    );
    let clipped_tile = vec4<f32>(
        clip_min,
        max(vec2<f32>(0.0), clip_max - clip_min),
    );

    var v_pos = clipped_tile.xy + corner * clipped_tile.zw;

    if bool(input.snap) {
        v_pos = round(v_pos * globals.scale_factor) / globals.scale_factor;
    }

    let uv = input.texture_pos + (v_pos - tile.xy) / tile.zw * input.texture_scale;
    let uv_center = input.texture_pos + input.texture_scale / 2.0;
    let d = uv - uv_center;

    out.uv = vec2<f32>(
        d.x * cos_r - d.y * sin_r,
        d.x * sin_r + d.y * cos_r,
    ) + uv_center;

    let normalized = vec2<f32>(
        v_pos.x / globals.size.x,
        v_pos.y / globals.size.y,
    );

    out.position = vec4<f32>(
        normalized.x * 2.0 - 1.0,
        1.0 - normalized.y * 2.0,
        0.0,
        1.0,
    );

    out.local_position = v_pos * globals.scale_factor;
    out.clip_bounds = globals.scale_factor * input.clip_bounds;
    out.border_radius = globals.scale_factor
        * min(
            input.border_radius,
            vec4<f32>(min(input.clip_bounds.z, input.clip_bounds.w) / 2.0),
        );
    out.texture_bounds = vec4<f32>(
        input.texture_pos,
        input.texture_pos + input.texture_scale,
    );
    out.opacity = input.opacity;

    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let fragment = input.local_position;
    let position = input.clip_bounds.xy;
    let scale = input.clip_bounds.zw;

    let d = rounded_box_sdf(
        2.0 * (fragment - position - scale / 2.0),
        scale,
        input.border_radius * 2.0,
    ) / 2.0;

    let antialias = clamp(1.0 - d, 0.0, 1.0);
    let inside = all(input.uv >= input.texture_bounds.xy)
        && all(input.uv <= input.texture_bounds.zw);

    return textureSample(u_texture, u_sampler, input.uv)
        * vec4<f32>(1.0, 1.0, 1.0, antialias * input.opacity * f32(inside));
}

fn rounded_box_sdf(p: vec2<f32>, size: vec2<f32>, corners: vec4<f32>) -> f32 {
    var box_half = select(corners.yz, corners.xw, p.x > 0.0);
    var corner = select(box_half.y, box_half.x, p.y > 0.0);
    var q = abs(p) - size + corner;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - corner;
}
"#;

/// GPU resources cached for one live cast handle.
struct TextureEntry {
    /// Shared liveness flag used to evict dead handles from the cache.
    alive: Arc<AtomicBool>,
    /// Texture storing the latest uploaded frame.
    texture: wgpu::Texture,
    /// Bind group referencing the texture view.
    bind_group: wgpu::BindGroup,
    /// Last frame generation uploaded into the texture.
    last_uploaded_generation: u64,
    /// Width of the current texture in pixels.
    width: u32,
    /// Height of the current texture in pixels.
    height: u32,
}

/// One prepared draw payload for one live image in the current frame.
struct PreparedView {
    /// Handle identifier used to fetch the texture bind group at draw time.
    texture_handle_id: u64,
    /// Uniform buffer kept alive for the prepared constants bind group.
    uniforms_buffer: wgpu::Buffer,
    /// Instance buffer holding the copied image-style draw payload.
    instances_buffer: wgpu::Buffer,
    /// Constants bind group containing uniforms and the chosen sampler.
    constants_bind_group: wgpu::BindGroup,
}

impl<Renderer> LiveRasterRenderer for PrimitiveLiveRasterRenderer<'_, Renderer>
where
    Renderer: advanced::Renderer + PrimitiveRenderer,
{
    /// Forwards one semantic live image to the cached primitive path used by
    /// `iced_wgpu`, clipping it through the current layer like the stock image
    /// widget does.
    fn draw_live_image(&mut self, image: LiveImage, bounds: Rectangle, clip_bounds: Rectangle) {
        let primitive = CastViewPrimitive::from_live_image(image, bounds, clip_bounds);

        self.renderer.with_layer(clip_bounds, |renderer| {
            PrimitiveRenderer::draw_primitive(renderer, bounds, primitive);
        });
    }
}

/// Key describing one prepared live-image draw payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct PreparedViewKey {
    /// Handle identifier used to fetch the live texture bind group.
    handle_id: u64,
    /// Filter method requested by the widget.
    filter_method: u8,
    /// Drawing width in logical pixels.
    width_bits: u32,
    /// Drawing height in logical pixels.
    height_bits: u32,
    /// Clip x in logical pixels, relative to the primitive origin.
    clip_x_bits: u32,
    /// Clip y in logical pixels, relative to the primitive origin.
    clip_y_bits: u32,
    /// Clip width in logical pixels.
    clip_width_bits: u32,
    /// Clip height in logical pixels.
    clip_height_bits: u32,
    /// Border radius in logical pixels.
    border_radius_bits: [u32; 4],
    /// Opacity requested by the widget.
    opacity_bits: u32,
    /// Rotation requested by the widget.
    rotation_bits: u32,
    /// Whether snapping is enabled.
    snap: bool,
}

impl PreparedViewKey {
    /// Builds one key from the primitive draw parameters.
    fn new(primitive: &CastViewPrimitive) -> Self {
        Self {
            handle_id: primitive.handle_id,
            filter_method: match primitive.filter_method {
                FilterMethod::Nearest => 0,
                FilterMethod::Linear => 1,
            },
            width_bits: primitive.size.width.to_bits(),
            height_bits: primitive.size.height.to_bits(),
            clip_x_bits: primitive.clip_bounds.x.to_bits(),
            clip_y_bits: primitive.clip_bounds.y.to_bits(),
            clip_width_bits: primitive.clip_bounds.width.to_bits(),
            clip_height_bits: primitive.clip_bounds.height.to_bits(),
            border_radius_bits: [
                primitive.border_radius.top_left.to_bits(),
                primitive.border_radius.top_right.to_bits(),
                primitive.border_radius.bottom_right.to_bits(),
                primitive.border_radius.bottom_left.to_bits(),
            ],
            opacity_bits: primitive.opacity.to_bits(),
            rotation_bits: primitive.rotation.radians().0.to_bits(),
            snap: primitive.snap,
        }
    }
}

/// Shared renderer state for all cast primitives.
pub(crate) struct CastViewPipeline {
    /// Render pipeline used to draw image-like quads.
    pipeline: wgpu::RenderPipeline,
    /// Bind-group layout shared by all live textures.
    texture_layout: wgpu::BindGroupLayout,
    /// Bind-group layout shared by uniforms and samplers.
    constants_layout: wgpu::BindGroupLayout,
    /// Nearest-neighbor sampler matching `iced` image rendering.
    nearest_sampler: wgpu::Sampler,
    /// Linear sampler matching `iced` image rendering.
    linear_sampler: wgpu::Sampler,
    /// Persistent texture cache keyed by handle identifier.
    texture_entries: BTreeMap<u64, TextureEntry>,
    /// Per-frame prepared draw payloads keyed by semantic draw parameters.
    prepared_views: BTreeMap<PreparedViewKey, PreparedView>,
}

impl Pipeline for CastViewPipeline {
    /// Builds the pipeline once for the current target format.
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("iced_live_cast nearest sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            min_filter: wgpu::FilterMode::Nearest,
            mag_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("iced_live_cast linear sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let constants_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("iced_live_cast constants layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<Uniforms>() as u64
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("iced_live_cast texture layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("iced_live_cast pipeline layout"),
            push_constant_ranges: &[],
            bind_group_layouts: &[&constants_layout, &texture_layout],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("iced_live_cast image shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("iced_live_cast pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array!(
                        0 => Float32x2,
                        1 => Float32x4,
                        2 => Float32x4,
                        3 => Float32x4,
                        4 => Float32,
                        5 => Float32,
                        6 => Float32x2,
                        7 => Float32x2,
                        8 => Uint32,
                    ),
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            texture_layout,
            constants_layout,
            nearest_sampler,
            linear_sampler,
            texture_entries: BTreeMap::new(),
            prepared_views: BTreeMap::new(),
        }
    }

    /// Drops cache entries once their handles have died and clears per-frame
    /// payloads.
    fn trim(&mut self) {
        self.texture_entries.retain(|_, entry| {
            if entry.alive.load(Ordering::SeqCst) {
                true
            } else {
                entry.texture.destroy();
                false
            }
        });

        self.prepared_views.clear();
    }
}

impl CastViewPipeline {
    /// Uploads one frame into the cache entry for the given handle.
    fn upload_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        primitive: &CastViewPrimitive,
    ) {
        let frame = &primitive.frame;
        let needs_recreate = needs_texture_recreate(
            self.texture_entries
                .get(&primitive.handle_id)
                .map(|entry| (entry.width, entry.height)),
            frame,
        );

        if needs_recreate {
            self.texture_entries.remove(&primitive.handle_id);
            self.texture_entries.insert(
                primitive.handle_id,
                create_texture_entry(device, &self.texture_layout, primitive, frame),
            );
        }

        let Some(entry) = self.texture_entries.get_mut(&primitive.handle_id) else {
            return;
        };

        if entry.last_uploaded_generation == primitive.generation {
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

        entry.last_uploaded_generation = primitive.generation;
    }

    /// Builds the copied image-style draw payload for one live primitive.
    fn prepare_view(
        &mut self,
        device: &wgpu::Device,
        viewport: &iced_wgpu::graphics::Viewport,
        primitive: &CastViewPrimitive,
    ) {
        let uniforms = Uniforms::new(viewport.scale_factor(), primitive.size);
        let uniforms_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("iced_live_cast uniforms buffer"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let instances = [Instance::from_primitive(primitive)];
        let instances_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("iced_live_cast instance buffer"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let sampler = match primitive.filter_method {
            FilterMethod::Nearest => &self.nearest_sampler,
            FilterMethod::Linear => &self.linear_sampler,
        };

        let constants_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("iced_live_cast constants bind group"),
            layout: &self.constants_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        self.prepared_views.insert(
            PreparedViewKey::new(primitive),
            PreparedView {
                texture_handle_id: primitive.handle_id,
                uniforms_buffer,
                instances_buffer,
                constants_bind_group,
            },
        );
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
    pub frame: Frame,
    /// Frame generation associated with the snapshot.
    pub generation: u64,
    /// Sampling filter requested by the widget.
    pub filter_method: FilterMethod,
    /// Opacity requested by the widget.
    pub opacity: f32,
    /// Border radius requested by the widget.
    pub border_radius: border::Radius,
    /// Rotation requested by the widget.
    pub rotation: Rotation,
    /// Final image-like drawing size computed by the stock image widget.
    pub size: Size,
    /// Clip bounds relative to the primitive origin, in logical pixels.
    pub clip_bounds: Rectangle,
    /// Whether the widget requested pixel snapping.
    pub snap: bool,
}

impl CastViewPrimitive {
    /// Builds one primitive from the semantic live-image payload emitted by the
    /// widget layer.
    fn from_live_image(image: LiveImage, bounds: Rectangle, clip_bounds: Rectangle) -> Self {
        let local_clip = bounds
            .intersection(&clip_bounds)
            .map(|intersection| {
                Rectangle::new(
                    Point::new(intersection.x - bounds.x, intersection.y - bounds.y),
                    intersection.size(),
                )
            })
            .unwrap_or_else(|| Rectangle::with_size(Size::ZERO));

        Self {
            handle_id: image.key,
            alive: image.alive,
            frame: image.frame,
            generation: image.generation,
            filter_method: image.filter_method,
            opacity: image.opacity,
            border_radius: image.border_radius,
            rotation: Rotation::from(image.rotation.0),
            size: bounds.size(),
            clip_bounds: local_clip,
            snap: image.snap,
        }
    }
}

impl Primitive for CastViewPrimitive {
    type Pipeline = CastViewPipeline;

    /// Uploads the latest frame and prepares the copied image-style draw
    /// payload.
    fn prepare(
        &self,
        pipeline: &mut CastViewPipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        viewport: &iced_wgpu::graphics::Viewport,
    ) {
        pipeline.upload_texture(device, queue, self);
        pipeline.prepare_view(device, viewport, self);
    }

    /// Draws inside the render pass that `iced` already configured for this
    /// primitive.
    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        let Some(view) = pipeline.prepared_views.get(&PreparedViewKey::new(self)) else {
            return true;
        };
        let Some(texture) = pipeline.texture_entries.get(&view.texture_handle_id) else {
            return true;
        };

        let _keep_uniforms_alive = &view.uniforms_buffer;

        render_pass.set_pipeline(&pipeline.pipeline);
        render_pass.set_bind_group(0, &view.constants_bind_group, &[]);
        render_pass.set_bind_group(1, &texture.bind_group, &[]);
        render_pass.set_vertex_buffer(0, view.instances_buffer.slice(..));
        render_pass.draw(0..6, 0..1);

        true
    }
}

/// One cached texture entry for the current handle and frame shape.
fn create_texture_entry(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    primitive: &CastViewPrimitive,
    frame: &Frame,
) -> TextureEntry {
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
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("iced_live_cast texture bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });

    TextureEntry {
        alive: Arc::clone(&primitive.alive),
        texture,
        bind_group,
        last_uploaded_generation: u64::MAX,
        width: frame.width(),
        height: frame.height(),
    }
}

/// Returns the texture format used by every uploaded live frame.
fn texture_format() -> wgpu::TextureFormat {
    if iced_wgpu::graphics::color::GAMMA_CORRECTION {
        wgpu::TextureFormat::Bgra8UnormSrgb
    } else {
        wgpu::TextureFormat::Bgra8Unorm
    }
}

/// Returns whether one cached texture must be rebuilt for the supplied frame
/// geometry.
fn needs_texture_recreate(existing_dimensions: Option<(u32, u32)>, frame: &Frame) -> bool {
    existing_dimensions
        .map(|(width, height)| width != frame.width() || height != frame.height())
        .unwrap_or(true)
}

/// Uniform block describing one live image draw inside the current primitive
/// viewport.
#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
struct Uniforms {
    /// Primitive viewport size in logical pixels.
    size: [f32; 2],
    /// Current scale factor converting logical pixels to physical pixels.
    scale_factor: f32,
    /// Padding required for uniform alignment.
    _padding: f32,
}

impl Uniforms {
    /// Builds one uniform block from the current primitive size.
    fn new(scale_factor: f32, size: Size) -> Self {
        Self {
            size: [size.width.max(1.0), size.height.max(1.0)],
            scale_factor,
            _padding: 0.0,
        }
    }
}

/// One copied image-style instance payload for a single live draw.
#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
struct Instance {
    /// Center of the full tile in local logical coordinates.
    _center: [f32; 2],
    /// Clip bounds in local logical coordinates.
    _clip_bounds: [f32; 4],
    /// Border radius in local logical coordinates.
    _border_radius: [f32; 4],
    /// Tile bounds in local logical coordinates.
    _tile: [f32; 4],
    /// Clockwise rotation in radians.
    _rotation: f32,
    /// Opacity multiplier.
    _opacity: f32,
    /// Normalized texture origin.
    _texture_pos: [f32; 2],
    /// Normalized texture size.
    _texture_scale: [f32; 2],
    /// Whether the image should snap to the pixel grid.
    _snap: u32,
}

impl Instance {
    /// Builds one copied stock-image instance from the current live primitive.
    fn from_primitive(primitive: &CastViewPrimitive) -> Self {
        let center = [primitive.size.width / 2.0, primitive.size.height / 2.0];
        let clip_bounds = [
            primitive.clip_bounds.x,
            primitive.clip_bounds.y,
            primitive.clip_bounds.width,
            primitive.clip_bounds.height,
        ];
        let border_radius = primitive.border_radius.into();

        Self {
            _center: center,
            _clip_bounds: clip_bounds,
            _border_radius: border_radius,
            _tile: [0.0, 0.0, primitive.size.width, primitive.size.height],
            _rotation: primitive.rotation.radians().0,
            _opacity: primitive.opacity,
            _texture_pos: [0.0, 0.0],
            _texture_scale: [1.0, 1.0],
            _snap: primitive.snap as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{needs_texture_recreate, texture_format, Instance, Uniforms};
    use crate::Frame;
    use iced::border;
    use iced::widget::image::FilterMethod;
    use iced::{Point, Rectangle, Rotation, Size};
    use iced_wgpu::wgpu;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    /// Live frames should follow `iced`'s gamma-correction mode.
    #[test]
    fn live_frames_follow_iced_gamma_mode() {
        let expected = if iced_wgpu::graphics::color::GAMMA_CORRECTION {
            wgpu::TextureFormat::Bgra8UnormSrgb
        } else {
            wgpu::TextureFormat::Bgra8Unorm
        };

        assert_eq!(texture_format(), expected);
    }

    /// Reused cache entries should survive when the next frame keeps the same
    /// geometry.
    #[test]
    fn matching_frame_dimensions_reuse_the_existing_entry() {
        let frame = sample_frame(1280, 720);

        assert!(!needs_texture_recreate(Some((1280, 720)), &frame));
    }

    /// Texture entries should be rebuilt when the frame geometry changes.
    #[test]
    fn changed_frame_dimensions_force_entry_recreation() {
        let frame = sample_frame(1920, 1080);

        assert!(needs_texture_recreate(Some((1280, 720)), &frame));
    }

    /// Border radii should still scale to physical pixels.
    #[test]
    fn border_radius_scales_to_physical_pixels() {
        let radius = border::Radius::from(8.0);

        assert_eq!(border_radius_px(radius, 2.0), [16.0, 16.0, 16.0, 16.0]);
    }

    /// Uniforms should carry the logical size and scale factor.
    #[test]
    fn uniforms_use_logical_size_and_scale_factor() {
        let uniforms = Uniforms::new(2.0, Size::new(200.0, 100.0));

        assert_eq!(uniforms.size, [200.0, 100.0]);
        assert_eq!(uniforms.scale_factor, 2.0);
    }

    /// Copied image instances should describe the current live primitive in
    /// local coordinates.
    #[test]
    fn instances_use_local_clip_bounds() {
        let primitive = sample_primitive();
        let instance = Instance::from_primitive(&primitive);

        assert_eq!(instance._clip_bounds, [10.0, 20.0, 90.0, 60.0]);
        assert_eq!(instance._tile, [0.0, 0.0, 200.0, 100.0]);
    }

    /// Converts border radius values into physical pixels.
    fn border_radius_px(border_radius: border::Radius, scale_factor: f32) -> [f32; 4] {
        let radius = border_radius * scale_factor;

        [
            radius.top_left,
            radius.top_right,
            radius.bottom_right,
            radius.bottom_left,
        ]
    }

    /// Builds one primitive suitable for renderer tests.
    fn sample_primitive() -> super::CastViewPrimitive {
        super::CastViewPrimitive {
            handle_id: 2,
            alive: Arc::new(AtomicBool::new(true)),
            frame: sample_frame(320, 180),
            generation: 3,
            filter_method: FilterMethod::Linear,
            opacity: 0.75,
            border_radius: border::Radius::from(4.0),
            rotation: Rotation::default(),
            size: Size::new(200.0, 100.0),
            clip_bounds: Rectangle::new(Point::new(10.0, 20.0), Size::new(90.0, 60.0)),
            snap: true,
        }
    }

    /// Builds one sample frame for renderer tests.
    fn sample_frame(width: u32, height: u32) -> Frame {
        Frame::from_bgra(width, height, vec![0; width as usize * height as usize * 4])
            .expect("sample frame should validate")
    }
}
