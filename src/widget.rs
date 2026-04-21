//! Active `iced` view for cast handles.

use crate::handle::{CastHandle, ManualSource, Source};
use crate::render::CastViewPrimitive;
use iced::advanced::{self, layout, widget, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Vector};
use iced_wgpu::primitive::Renderer as PrimitiveRenderer;
use std::marker::PhantomData;
use std::sync::Arc;

/// How a live frame should fit within view bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FitMode {
    /// Show the whole frame, allowing letterboxing when needed.
    Contain,
    /// Fill the bounds, allowing cropping when needed.
    Cover,
}

/// Active widget that renders one [`CastHandle`].
pub struct CastView<'a, Message, S = ManualSource, Theme = iced::Theme, Renderer = iced::Renderer>
where
    S: Source,
    Renderer: PrimitiveRenderer,
{
    /// Handle rendered by the view.
    handle: CastHandle<S>,
    /// How the frame should fit inside the view bounds.
    fit_mode: FitMode,
    /// Requested widget width.
    width: Length,
    /// Requested widget height.
    height: Length,
    /// Optional mapper used to publish one message when a new source error appears.
    ///
    /// Returning `None` lets callers acknowledge an error without forcing a
    /// placeholder application message.
    on_error: Option<Box<dyn Fn(&S::Error) -> Option<Message> + 'a>>,
    /// Marker tying the widget to the app's message and renderer types.
    _marker: PhantomData<(Theme, Renderer)>,
}

/// Persistent widget state used to avoid re-publishing the same error.
#[derive(Debug, Default)]
struct CastViewState {
    /// Last error generation already published through `on_error`.
    last_error_generation: u64,
}

impl<'a, Message, S, Theme, Renderer> CastView<'a, Message, S, Theme, Renderer>
where
    S: Source,
    Renderer: PrimitiveRenderer,
{
    /// Builds one view for anything that exposes one shared [`CastHandle`].
    pub fn new(handle: impl AsRef<CastHandle<S>>) -> Self {
        Self {
            fit_mode: FitMode::Contain,
            handle: handle.as_ref().clone(),
            width: Length::Shrink,
            height: Length::Shrink,
            on_error: None,
            _marker: PhantomData,
        }
    }

    /// Sets the view width.
    pub fn width(self, width: impl Into<Length>) -> Self {
        Self {
            width: width.into(),
            ..self
        }
    }

    /// Sets the view height.
    pub fn height(self, height: impl Into<Length>) -> Self {
        Self {
            height: height.into(),
            ..self
        }
    }

    /// Sets the fit mode used to place the live frame.
    pub fn fit_mode(self, fit_mode: FitMode) -> Self {
        Self { fit_mode, ..self }
    }

    /// Maps one newly reported source error into an optional application message.
    pub fn on_error(self, on_error: impl Fn(&S::Error) -> Option<Message> + 'a) -> Self {
        Self {
            on_error: Some(Box::new(on_error)),
            ..self
        }
    }
}

impl<'a, Message, S, Theme, Renderer> Widget<Message, Theme, Renderer>
    for CastView<'a, Message, S, Theme, Renderer>
where
    S: Source,
    Renderer: PrimitiveRenderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let source_size = preview_size(self.handle.dimensions());
        let raw_size = limits.resolve(self.width, self.height, source_size);
        let fitted = fit_mode(self.fit_mode).fit(source_size, raw_size);
        let final_size = Size {
            width: match self.width {
                Length::Shrink => raw_size.width.min(fitted.width),
                _ => raw_size.width,
            },
            height: match self.height {
                Length::Shrink => raw_size.height.min(fitted.height),
                _ => raw_size.height,
            },
        };

        layout::Node::new(final_size)
    }

    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &advanced::renderer::Style,
        layout: advanced::Layout<'_>,
        _cursor: advanced::mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let source_size = preview_size(self.handle.dimensions());
        let fitted = fit_mode(self.fit_mode).fit(source_size, bounds.size());
        let scale = Vector::new(
            fitted.width / source_size.width.max(1.0),
            fitted.height / source_size.height.max(1.0),
        );
        let final_size = source_size * scale;
        let position = iced::Point::new(
            bounds.center_x() - final_size.width / 2.0,
            bounds.center_y() - final_size.height / 2.0,
        );
        let drawing_bounds = Rectangle::new(position, final_size);
        let primitive = CastViewPrimitive {
            handle_id: self.handle.inner.id,
            alive: Arc::clone(&self.handle.inner.alive),
            frame: self.handle.inner.snapshot(),
            generation: self.handle.inner.generation(),
        };

        let render = |renderer: &mut Renderer| {
            renderer.draw_primitive(drawing_bounds, primitive);
        };

        if fitted.width > bounds.width || fitted.height > bounds.height {
            renderer.with_layer(bounds, render);
        } else {
            render(renderer);
        }
    }

    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<CastViewState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(CastViewState::default())
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        _layout: advanced::Layout<'_>,
        _cursor: advanced::mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn advanced::Clipboard,
        shell: &mut advanced::Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<CastViewState>();

        if let Some(on_error) = self.on_error.as_ref() {
            let error_generation = self.handle.inner.error_generation();

            if error_generation != state.last_error_generation {
                if let Some(error) = self.handle.last_error() {
                    if let Some(message) = on_error(&error) {
                        shell.publish(message);
                    }
                }

                state.last_error_generation = error_generation;
            }
        }

        if let Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            if !self.handle.is_paused() && !self.handle.is_stopped() {
                shell.request_redraw_at(*now + self.handle.redraw_interval());
            }
        }
    }
}

impl<'a, Message, S, Theme, Renderer> From<CastView<'a, Message, S, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    S: Source,
    Message: 'a,
    Theme: 'a,
    Renderer: 'a + PrimitiveRenderer,
{
    fn from(widget: CastView<'a, Message, S, Theme, Renderer>) -> Self {
        Self::new(widget)
    }
}

/// Maps the public fit enum to `iced`'s layout helper.
fn fit_mode(mode: FitMode) -> iced::ContentFit {
    match mode {
        FitMode::Contain => iced::ContentFit::Contain,
        FitMode::Cover => iced::ContentFit::Cover,
    }
}

/// Returns the dimensions the view should fit.
fn preview_size(dimensions: Option<(u32, u32)>) -> Size {
    let (width, height) = dimensions.unwrap_or((16, 9));

    Size::new(width.max(1) as f32, height.max(1) as f32)
}

#[cfg(test)]
mod tests {
    use super::{fit_mode, preview_size};
    use crate::widget::FitMode;
    use iced::Size;

    /// `Contain` should keep the whole frame visible.
    #[test]
    fn contain_preserves_full_frame() {
        let source = Size::new(1920.0, 1080.0);
        let bounds = Size::new(1000.0, 1000.0);
        let fitted = fit_mode(FitMode::Contain).fit(source, bounds);

        assert_eq!(fitted.width, 1000.0);
        assert!(fitted.height < 1000.0);
    }

    /// `Cover` should fill the destination even when it crops.
    #[test]
    fn cover_fills_bounds() {
        let source = Size::new(1920.0, 1080.0);
        let bounds = Size::new(1000.0, 1000.0);
        let fitted = fit_mode(FitMode::Cover).fit(source, bounds);

        assert!(fitted.width > 1000.0 || fitted.height > 1000.0);
    }

    /// Preview sizing should follow the latest frame dimensions.
    #[test]
    fn preview_size_uses_frame_dimensions() {
        assert_eq!(preview_size(Some((1920, 1080))), Size::new(1920.0, 1080.0));
    }
}
