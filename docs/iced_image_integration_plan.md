# `iced` Image Integration Plan

## Goal

Reduce the amount of custom `CastView` code while preserving the one optimization
that matters for live casting:

- keep one GPU texture per cast handle
- upload new pixels when the frame generation changes
- do not route live frames through `iced`'s raster image cache

The target is **image-like behavior without image-handle caching**.

## Audit Summary

### Exposed APIs That Can Help

#### 1. `iced_widget::image::layout`

`iced_widget::image::layout` is public and already contains the layout contract
we want:

- `width`
- `height`
- `expand`
- `content_fit`
- `rotation`
- source-region-aware sizing

It only needs a renderer that implements `iced_core::image::Renderer` so it can
call `measure_image`.

This means we can reuse the stock layout behavior with one tiny adapter whose
only real job is returning frame dimensions.

#### 2. `iced_core::Renderer::with_layer`

`with_layer` is public on the core renderer trait and is the normal clipping
mechanism used by widgets when their drawing can extend outside widget bounds.

We should keep using it instead of managing scissoring logic in `CastView`.

#### 3. `iced_wgpu::primitive::Renderer::draw_primitive`

`draw_primitive` is the lowest exposed `wgpu` seam for custom live rendering.

This already gives us the right backend behavior:

- primitive batching
- lazy pipeline initialization
- renderer-owned pipeline storage
- renderer-owned per-frame prepare/draw orchestration

This is the correct draw seam for live frames when we do not want raster-image
cache participation.

#### 4. `iced_wgpu::primitive::Primitive::draw`

The primitive API itself prefers `draw(...)` over `render(...)` when possible.

That matters because `draw(...)` lets `iced` keep ownership of the existing
render pass, viewport, and scissor setup. It is the lighter integration point.

If our live primitive can be expressed inside the existing pass, we should move
there and stop creating a dedicated pass in `render(...)`.

### Exposed APIs That Do Not Solve The Live Path

#### 1. `iced_widget::image::draw`

`iced_widget::image::draw` is public, but it calls `renderer.draw_image(...)`.
That draw path is tied to the stock raster image pipeline.

For the `wgpu` backend, `draw_image` records one raster image into the current
layer, and later the renderer prepares that image through its raster image
cache.

This is correct for ordinary images, but it is the wrong cache model for live
cast frames.

#### 2. `iced_core::image::Renderer`

`iced::Renderer` already implements `iced_core::image::Renderer` with
`Handle = iced_core::image::Handle`.

That means we cannot add another implementation for `iced::Renderer` using
`Handle = Frame`.

Even if that coherence restriction did not exist, the stock image draw path
still leads into the raster cache keyed by `image::Handle::id()`.

#### 3. `iced_widget::shader::Shader`

The shader widget is only a convenience wrapper around `draw_primitive`.

It does not remove the custom primitive path, and it does not give us the stock
image renderer semantics for free. It is not the seam we need.

## What We Can Reuse Exactly

### Reuse exactly

- `iced_widget::image::layout`
- `iced` public image-widget builder surface
- `with_layer`
- `ContentFit`
- `Rotation`
- `border::Radius`
- `FilterMethod` semantics

### Reuse conceptually, but not by direct call

- `iced_widget::image::draw`
- stock image clipping behavior
- stock image draw bounds behavior

These pieces cannot be used directly without entering the image-handle cache
path. They can only be matched semantically.

## The Lean Plan

### 1. Use a measure-only image renderer adapter for layout

Create a small private adapter type whose only meaningful method is
`measure_image`.

Suggested shape:

```rust
struct FrameLayoutHandle {
    width: u32,
    height: u32,
}

struct FrameLayoutRenderer;
```

Implement `iced_core::image::Renderer` for `FrameLayoutRenderer`:

- `type Handle = FrameLayoutHandle`
- `measure_image` returns the stored dimensions
- `load_image` is `unreachable!()`
- `draw_image` is `unreachable!()`

Then `CastView::layout` can call `iced_widget::image::layout(...)` directly.

This removes all custom layout math while staying outside the raster cache.

### 2. Keep the draw path on primitives

Do not call:

- `iced_widget::image::draw`
- `renderer.draw_image(...)`

Keep rendering on the primitive path:

- one texture entry per `CastHandle`
- upload on generation change
- no frame hashing
- no `image::Handle`

This preserves the only cache behavior we actually want: stable texture reuse.

### 3. Narrow the widget-to-renderer contract

`CastView` should not know about the full primitive backend surface.

Introduce a tiny crate-local renderer trait, for example:

```rust
trait LiveRasterRenderer: iced::advanced::Renderer {
    fn draw_live_raster(&mut self, primitive: CastViewPrimitive, bounds: Rectangle);
}
```

Implement it for `iced::Renderer` by forwarding to `draw_primitive`.

This keeps `CastView` clean and makes the backend seam explicit without forcing
callers to care about primitive internals.

### 4. Prefer `Primitive::draw(...)` over `Primitive::render(...)`

The current live primitive should be simplified to use `draw(...)` if at all
possible.

That lets `iced` own:

- pass creation
- viewport setup
- scissor setup
- command encoder orchestration

This does not remove the custom primitive path, but it does reduce how much
render-pass management we own.

### 5. Keep one small local copy of image draw-bounds math

The one part `iced` does not expose cleanly is the private `drawing_bounds`
helper used by the image widget.

We have two realistic options:

- keep one local copy of that pure geometry helper
- upstream one public helper to `iced` later

For now, the least risky move is to keep that helper local and keep it as small
as possible.

This is the only remaining duplication worth tolerating.

### 6. Match stock image behavior at the API surface

`CastView` should keep the same user-facing knobs as `iced::Image`:

- `width`
- `height`
- `expand`
- `content_fit`
- `filter_method`
- `rotation`
- `opacity`
- `scale`
- `crop`
- `border_radius`

And add only the live-cast-specific behavior:

- `on_error(...)`
- active redraw scheduling

## What This Plan Avoids

This plan avoids all of these dead ends:

- stock `image::Handle` churn for live frames
- trying to re-implement `iced_core::image::Renderer` for `iced::Renderer`
- trying to use `Shader` as if it removed primitive work
- trying to fork `iced_wgpu` image internals just to avoid one small layout copy

## Recommended Next Implementation Order

1. Replace `CastView::layout` with a call to `iced_widget::image::layout(...)`
   through the measure-only adapter.
2. Introduce one crate-local `LiveRasterRenderer` trait and implement it for
   `iced::Renderer`.
3. Move the live primitive from `render(...)` to `draw(...)` if the current
   transform and clipping needs allow it.
4. Keep the primitive renderer path, but make it consume the same public widget
   fields as `iced::Image`.
5. Reduce the local draw-bounds helper until it contains only the private image
   geometry that `iced` does not expose.
6. Keep the side-by-side probe and use it as the visual parity check.

## Bottom Line

The best lightweight architecture is:

- **layout from exposed `iced` image APIs**
- **draw from our live primitive path**

That is the closest we can get to stock image behavior without giving up the
no-frame-cache requirement.
