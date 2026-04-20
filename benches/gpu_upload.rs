//! Benchmarks for the GPU upload path used by the live cast renderer.

mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use iced_wgpu::wgpu;
use pollster::block_on;
use std::sync::OnceLock;
use support::{bgra_pixels, FRAME_PROFILES};

/// Shared GPU state used by the upload benchmarks.
struct GpuHarness {
    /// Device used to allocate textures for upload measurements.
    device: wgpu::Device,
    /// Queue used to submit texture writes.
    queue: wgpu::Queue,
}

/// Returns one lazily initialized GPU harness when the current machine provides an adapter.
fn gpu_harness() -> Option<&'static GpuHarness> {
    static GPU: OnceLock<Option<GpuHarness>> = OnceLock::new();

    GPU.get_or_init(|| block_on(init_gpu())).as_ref()
}

/// Builds one headless GPU device for upload benchmarks.
async fn init_gpu() -> Option<GpuHarness> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .ok()?;

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("iced_live_cast gpu bench device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        })
        .await
        .ok()?;

    Some(GpuHarness { device, queue })
}

/// Builds one texture matching the frame geometry for the current benchmark profile.
fn texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("iced_live_cast gpu bench texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

/// Benchmarks reusing one texture while uploading fresh BGRA pixels into it.
fn bench_reused_texture_upload(c: &mut Criterion) {
    let Some(gpu) = gpu_harness() else {
        return;
    };

    let mut group = c.benchmark_group("gpu_upload/reused_texture");

    for profile in FRAME_PROFILES {
        let pixels = bgra_pixels(profile);
        let texture = texture(&gpu.device, profile.width, profile.height);
        let bytes_per_row = profile.width * 4;

        group.throughput(Throughput::Bytes(pixels.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("enqueue", profile.name),
            &pixels,
            |b, pixels| {
                b.iter(|| {
                    gpu.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        pixels,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(bytes_per_row),
                            rows_per_image: Some(profile.height),
                        },
                        wgpu::Extent3d {
                            width: profile.width,
                            height: profile.height,
                            depth_or_array_layers: 1,
                        },
                    );
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks the more expensive path that recreates one texture before each upload.
fn bench_texture_recreation_and_upload(c: &mut Criterion) {
    let Some(gpu) = gpu_harness() else {
        return;
    };

    let mut group = c.benchmark_group("gpu_upload/recreate_texture");

    for profile in FRAME_PROFILES {
        let pixels = bgra_pixels(profile);
        let bytes_per_row = profile.width * 4;

        group.throughput(Throughput::Bytes(pixels.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("enqueue", profile.name),
            &pixels,
            |b, pixels| {
                b.iter(|| {
                    let texture = texture(&gpu.device, profile.width, profile.height);

                    gpu.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        pixels,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(bytes_per_row),
                            rows_per_image: Some(profile.height),
                        },
                        wgpu::Extent3d {
                            width: profile.width,
                            height: profile.height,
                            depth_or_array_layers: 1,
                        },
                    );
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    gpu_upload,
    bench_reused_texture_upload,
    bench_texture_recreation_and_upload
);
criterion_main!(gpu_upload);
