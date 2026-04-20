//! Benchmarks for converting frames and surfaces into `iced` image handles.

mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use support::{packed_bgra_frame, seeded_handle, FRAME_PROFILES};

/// Benchmarks direct frame-to-handle conversion.
fn bench_frame_to_handle(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_handles/frame_to_handle");

    for profile in FRAME_PROFILES {
        let frame = packed_bgra_frame(profile);
        group.throughput(Throughput::Bytes(profile.packed_len() as u64));
        group.bench_with_input(
            BenchmarkId::new("full_frame", profile.name),
            &frame,
            |b, frame| {
                b.iter(|| black_box(frame).to_handle());
            },
        );
    }

    group.finish();
}

/// Benchmarks surface snapshot cloning.
fn bench_surface_snapshot(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_handles/surface_snapshot");

    for profile in FRAME_PROFILES {
        let handle = seeded_handle(profile);
        group.throughput(Throughput::Bytes(profile.packed_len() as u64));
        group.bench_with_input(
            BenchmarkId::new("snapshot", profile.name),
            &handle,
            |b, handle| {
                b.iter(|| black_box(handle).snapshot().expect("frame should exist"));
            },
        );
    }

    group.finish();
}

criterion_group!(frame_handles, bench_frame_to_handle, bench_surface_snapshot);
criterion_main!(frame_handles);
