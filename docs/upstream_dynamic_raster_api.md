# Draft: First-Class Dynamic Raster Support In `iced`

## Suggested Issue Title

Add first-class support for dynamic raster images without stock image-cache churn

## Short Summary

`iced` already has a strong image widget and a solid `wgpu` image renderer, but
the current public seams assume raster images are mostly static and therefore
fit naturally into the stock image cache.

That assumption breaks down for crates that render mutable image streams:

- live screen casting
- video playback
- camera feeds
- remote desktop clients
- any widget that updates image pixels continuously

Today, ecosystem crates can get correct image geometry from the public image
widget APIs, but they cannot reuse the stock raster draw path without going
through `image::Handle`-driven cache behavior.

This leads to duplicated custom pipelines in crates like:

- `iced_live_cast`
- `iced_video_player`

The missing abstraction is not "draw images" in general. `iced` already does
that well. The missing abstraction is:

- draw an image-like thing
- with a stable identity
- whose pixels change over time
- without forcing every frame into the stock raster-image cache model

## Problem

The current public path looks roughly like this:

1. `iced_widget::image::Image` computes image layout and draw parameters
2. `Renderer::draw_image(...)` forwards to `iced_wgpu` layer batching
3. `iced_wgpu` later prepares the raster through the image cache

This is exactly right for static or mostly-static images.

It is not the right model for a live source that wants:

- one stable GPU texture per stream
- pixel uploads only when the stream generation changes
- no per-frame image-handle churn

If a crate uses fresh `image::Handle`s for live frames, it tends to fight the
stock cache. If it avoids that path, it has to re-implement a custom mutable
texture pipeline.

## Why This Matters

The ecosystem already has at least two separate crates solving the same missing
piece in different ways:

- live capture widgets
- video widgets

Both want to look and behave like normal `iced` images:

- same sizing rules
- same `ContentFit`
- same filter semantics
- same clipping
- same rotation/opacity/border-radius behavior

But both need a different backing storage model from static images.

That suggests a real framework-level gap instead of one-off crate-specific
hacks.

## Proposed Direction

Add first-class support for **dynamic raster images** as a sibling to the
current static raster image path.

The core idea is:

- keep the existing image widget geometry contract
- keep the existing image rendering semantics
- add a distinct renderer/data path for mutable raster sources

## Proposed API Shape

This is intentionally additive and high-level.

### 1. A dynamic image payload in `iced_core`

Something conceptually like:

```rust
pub struct DynamicImage {
    pub id: DynamicImageId,
    pub generation: u64,
    pub size: Size<u32>,
    pub filter_method: FilterMethod,
    pub rotation: Radians,
    pub border_radius: border::Radius,
    pub opacity: f32,
    pub snap: bool,
}
```

Important notes:

- `id` is stable across frames of the same stream
- `generation` changes when pixels change
- `size` is explicit
- image-style presentation knobs stay image-like
- this is **not** a stock `image::Handle`

The framework may want a different exact name or field set, but those are the
important semantics.

### 2. A renderer trait for dynamic rasters

Something conceptually like:

```rust
pub trait DynamicImageRenderer: Renderer {
    fn draw_dynamic_image(
        &mut self,
        image: DynamicImage,
        bounds: Rectangle,
        clip_bounds: Rectangle,
    );
}
```

This mirrors the existing image draw contract closely so widgets can keep using
the same geometry and only change the backing raster source type.

### 3. A backend upload hook or source object

The renderer also needs a way to obtain the current pixels for a dynamic image.
There are a few ways to shape this:

#### Option A: callback-based source

```rust
pub trait DynamicImageSource {
    fn size(&self) -> Size<u32>;
    fn generation(&self) -> u64;
    fn upload(&self, encoder: &mut dyn DynamicImageUploader);
}
```

#### Option B: byte-provider API

```rust
pub trait DynamicImageSource {
    fn size(&self) -> Size<u32>;
    fn generation(&self) -> u64;
    fn bytes(&self) -> DynamicImageBytes<'_>;
}
```

#### Option C: widget-owned prepare data

The widget or adapter resolves the latest frame into a backend-neutral payload
before calling `draw_dynamic_image(...)`.

I think **Option B** is the most likely low-friction first step if `iced`
wants to support CPU-backed mutable rasters first.

## Why This Should Be Separate From `image::Handle`

Because the storage model is different.

`image::Handle` works well when an image can be identified and cached as one
stable asset.

A dynamic raster wants:

- one stream identity
- many generations
- stable GPU texture reuse
- uploads instead of cache replacement

Trying to force both use cases through one public abstraction makes both the API
and the backend behavior harder to reason about.

## What `iced_wgpu` Would Likely Do

This is the part that makes the proposal attractive: `iced_wgpu` already has
most of the pieces.

It already knows how to:

- batch image-like draws in layers
- apply image-style clipping, transformation, filtering, and border radius
- keep renderer-owned state around

The missing backend-side behavior is mainly:

- maintain one texture per dynamic image id
- update texture contents only when generation changes
- route draw requests through the same image-style batching semantics
  without going through the stock static-image cache

So this does **not** require inventing a new visual model. It mostly requires
exposing a new storage/update model.

## Why This Helps `iced_live_cast`

`iced_live_cast` wants:

- stock image sizing and geometry behavior
- one mutable texture per cast handle
- uploads when the frame generation changes
- no stock frame-cache churn

That is exactly the use case above.

## Why This Helps `iced_video_player`

`iced_video_player` has a different media pipeline, but the end of the pipeline
has the same need:

- draw continuously changing frame data
- preserve image-like widget behavior
- avoid reinventing the entire mutable-raster draw path

Even if video keeps specialized decode-side logic, a shared dynamic raster draw
abstraction could reduce duplicated renderer code.

## What This Proposal Does *Not* Ask For

This proposal is intentionally **not** asking for:

- exposing raw `wgpu` internals publicly
- exposing `iced_wgpu::layer::Layer`
- exposing the stock image cache directly
- exposing WGSL or pipeline internals
- making widgets depend on backend-specific APIs

The goal is a small new public abstraction, not more leakage of backend
internals.

## Minimal Initial Scope

If the full dynamic-image story feels too broad, a smaller first step could be:

1. Add a new core-level dynamic raster type and renderer trait
2. Implement it only for `iced_wgpu`
3. Leave `iced_widget` support to ecosystem crates first

That would already let crates like `iced_live_cast` and `iced_video_player`
stop owning as much custom backend code.

## Alternative Smaller Step

If maintainers want an even smaller experiment, another plausible first PR
could be:

- expose a public helper for image draw-bounds calculation
- expose a higher-level image-like primitive draw seam that bypasses
  `image::Handle` caching

That would not solve the whole problem as cleanly, but it would still reduce a
lot of duplication in downstream crates.

## Open Questions

These are the questions I would expect maintainers to care about:

1. Should dynamic raster support live in `iced_core`, `iced_graphics`, or only
   in backend crates first?
2. Should the source model be CPU-byte-based, upload-callback-based, or
   something else?
3. Should `iced_widget::image::Image` itself eventually support dynamic
   sources, or should this remain a separate widget path?
4. How much of the existing static-image batching can be reused cleanly without
   over-coupling the two models?

## Suggested Closing Ask

Would maintainers be open to a focused design discussion on first-class dynamic
raster support?

If the general direction makes sense, I think a follow-up PR could target the
smallest additive surface that:

- preserves stock image semantics
- adds stable-id + generation support
- lets mutable image streams avoid the static raster cache model
