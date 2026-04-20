# iced_live_cast

`iced_live_cast` is a small `iced`-focused crate for live display casting.

It gives you three layers:

- `CastView` for actively rendering a live handle inside `iced`
- `CastHandle` for callers that already own frames and want to push them
- `DisplayCapture` for OS-backed display capture on macOS and Windows

The crate keeps the hot path out of your application message loop:

- sources push frames into a shared handle
- the widget owns redraw scheduling
- the renderer reuses one GPU texture per handle instead of rebuilding per frame

## Features

- active `iced` widget with redraw scheduling
- typed source markers and typed runtime errors
- BGRA upload path tuned for OS display capture
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
```
