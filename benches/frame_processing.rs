//! Benchmarks for frame conversion work.

mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use support::{packed_bgra_frame, packed_rgba_frame, padded_bgra_frame, FRAME_PROFILES};

/// Benchmarks frame-to-RGBA conversion across the supported source layouts.
fn bench_rgba_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_processing/rgba_pixels");

    for profile in FRAME_PROFILES {
        group.throughput(Throughput::Bytes(profile.packed_len() as u64));

        let rgba = packed_rgba_frame(profile);
        group.bench_with_input(
            BenchmarkId::new("packed_from_rgba", profile.name),
            &rgba,
            |b, frame| {
                b.iter(|| black_box(frame).rgba_pixels());
            },
        );

        let bgra = packed_bgra_frame(profile);
        group.bench_with_input(
            BenchmarkId::new("packed_bgra", profile.name),
            &bgra,
            |b, frame| {
                b.iter(|| black_box(frame).rgba_pixels());
            },
        );

        let padded = padded_bgra_frame(profile);
        group.bench_with_input(
            BenchmarkId::new("padded_bgra", profile.name),
            &padded,
            |b, frame| {
                b.iter(|| black_box(frame).rgba_pixels());
            },
        );
    }

    group.finish();
}

criterion_group!(frame_processing, bench_rgba_conversion);
criterion_main!(frame_processing);
