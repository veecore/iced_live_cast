//! Benchmarks for the public cast-handle update API.

mod support;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use iced_live_cast::{CastHandle, Frame};
use std::hint::black_box;
use support::{bgra_pixels, packed_bgra_frame, rgba_pixels, FRAME_PROFILES};

/// Benchmarks presenting already-validated frames.
fn bench_present_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("cast_handle_updates/present_frame");

    for profile in FRAME_PROFILES {
        let handle: CastHandle = CastHandle::new();
        let frame = packed_bgra_frame(profile);
        group.bench_with_input(
            BenchmarkId::new("prebuilt_bgra", profile.name),
            &handle,
            |b, handle| {
                b.iter(|| black_box(handle).present(black_box(frame.clone())));
            },
        );
    }

    group.finish();
}

/// Benchmarks constructing and presenting validated frames in one step.
fn bench_construct_and_present(c: &mut Criterion) {
    let mut group = c.benchmark_group("cast_handle_updates/construct_and_present");

    for profile in FRAME_PROFILES {
        let handle: CastHandle = CastHandle::new();
        let rgba = rgba_pixels(profile);
        let bgra = bgra_pixels(profile);

        group.bench_with_input(
            BenchmarkId::new("rgba", profile.name),
            &handle,
            |b, handle| {
                b.iter(|| {
                    black_box(handle).present(
                        Frame::from_rgba_owned(
                            profile.width,
                            profile.height,
                            black_box(rgba.clone()),
                        )
                        .expect("frame should validate"),
                    )
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("bgra", profile.name),
            &handle,
            |b, handle| {
                b.iter(|| {
                    black_box(handle).present(
                        Frame::from_bgra(profile.width, profile.height, black_box(bgra.clone()))
                            .expect("frame should validate"),
                    )
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    cast_handle_updates,
    bench_present_frame,
    bench_construct_and_present
);
criterion_main!(cast_handle_updates);
