//! Benchmarks for building validated frame values.

mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use iced_live_cast::Frame;
use std::hint::black_box;
use support::{bgra_pixels, padded_bgra_parts, rgba_pixels, FRAME_PROFILES};

/// Benchmarks construction of tightly packed RGBA frames.
fn bench_from_rgba(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_construction/from_rgba");

    for profile in FRAME_PROFILES {
        let pixels = rgba_pixels(profile);
        group.throughput(Throughput::Bytes(pixels.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("packed", profile.name),
            &pixels,
            |b, pixels| {
                b.iter(|| {
                    Frame::from_rgba_owned(profile.width, profile.height, black_box(pixels.clone()))
                        .expect("frame should validate")
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks construction of tightly packed BGRA frames.
fn bench_from_bgra(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_construction/from_bgra");

    for profile in FRAME_PROFILES {
        let pixels = bgra_pixels(profile);
        group.throughput(Throughput::Bytes(pixels.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("packed", profile.name),
            &pixels,
            |b, pixels| {
                b.iter(|| {
                    Frame::from_bgra(profile.width, profile.height, black_box(pixels.clone()))
                        .expect("frame should validate")
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks construction of BGRA frames with explicit padded row strides.
fn bench_from_padded_parts(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_construction/from_parts");

    for profile in FRAME_PROFILES {
        let (bytes_per_row, pixels) = padded_bgra_parts(profile);
        group.throughput(Throughput::Bytes(pixels.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("padded_bgra", profile.name),
            &pixels,
            |b, pixels| {
                b.iter(|| {
                    Frame::new(
                        profile.width,
                        profile.height,
                        bytes_per_row,
                        black_box(pixels.clone()),
                    )
                    .expect("frame should validate")
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    frame_construction,
    bench_from_rgba,
    bench_from_bgra,
    bench_from_padded_parts
);
criterion_main!(frame_construction);
