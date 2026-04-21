# iced_live_cast

Blazing-fast cross-platform display casting for `iced`.

`iced_live_cast` is the crate you reach for when you want a live preview in an
`iced` app without routing frames through your application's message loop.

It gives you three layers:

- `CastView` for actively rendering a live handle inside `iced`
- `CastHandle` for callers that already own frames and want to push them
- `DisplayCapture` for OS-backed display capture on macOS and Windows

The hot path stays small:

- sources push frames into a shared handle
- the widget owns redraw scheduling
- the renderer reuses one GPU texture per handle instead of rebuilding per frame

## Why this crate exists

If you already have frames, use `CastHandle`.

If you want the OS to produce frames for you, use `DisplayCapture`.

If you just want something on screen, use `CastView`.

That is the whole pitch.

## What you get

- an active `iced` widget with redraw scheduling
- a reusable GPU texture per live handle
- typed source markers and typed runtime errors
- a BGRA upload path tuned for display capture
- macOS support through `screencapturekit`
- Windows support through `windows-capture`

## Quick start

```rust
use iced::widget::container;
use iced::{Element, Length};
use iced_live_cast::{CastView, Display, DisplayCapture};
use std::num::NonZeroU32;

fn preview() -> Result<Element<'static, ()>, Box<dyn std::error::Error>> {
    let display = Display::new(NonZeroU32::new(1).expect("display id is non-zero"));
    let capture = DisplayCapture::start(display)?;

    Ok(container(
        CastView::new(&capture)
            .width(Length::Fill)
            .height(400),
    )
    .into())
}
```

For the full story, run:

```bash
cargo run --example basic -- 1
```

That example uses the built-in display source.

## Manual sources

If your frames come from somewhere other than the built-in display source, use
`CastHandle` directly:

```rust
use iced_live_cast::{CastHandle, Frame};

let handle = CastHandle::new();
let frame = Frame::from_bgra(1280, 720, vec![0; 1280 * 720 * 4])?;
handle.present(frame);
```

You can then render the same handle with `CastView::new(&handle)`.

If you want a full running example for that path, run:

```bash
cargo run --example manual_push
```

And here is that manual source path running for real:

![Manual push example](assets/manual_push_demo.gif)

## Benchmarks

Quick local numbers from a short Criterion run on Apple silicon with 1080p frames:

| Benchmark | Size | Result |
| --- | --- | --- |
| `frame_construction/from_rgba/packed` | 1080p | about `1.52 ms` |
| `frame_construction/from_bgra/packed` | 1080p | about `475 µs` |
| `frame_processing/rgba_pixels/packed_bgra` | 1080p | about `328 µs` |
| `frame_handles/frame_to_handle/full_frame` | 1080p | about `370 µs` |
| `cast_handle_updates/present_frame/prebuilt_bgra` | 1080p | about `19 ns` |
| `cast_handle_updates/construct_and_present/bgra` | 1080p | about `469 µs` |

The GPU upload bench is included too, but treat it as a renderer baseline rather
than a full end-to-end crate benchmark. It measures the same `wgpu` upload shape
the renderer uses, not the whole widget pipeline.

## Platform notes

- macOS requires Screen Recording permission from the OS before display capture
  can start.
- The crate uses `screencapturekit` on macOS and `windows-capture` on Windows.
- Linux support is not implemented yet.

## Development

Useful checks from the workspace root:

```bash
cargo check --lib --examples --benches
cargo test --lib
cargo bench --no-run
```
