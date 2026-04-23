//! Active `iced` view for cast handles.

use crate::handle::{CastHandle, ManualSource, Source};
use crate::render::{LiveImage, LiveRasterRenderer, PrimitiveLiveRasterRenderer};
use iced::advanced::{self, image as core_image, layout, widget, Widget};
use iced::border;
use iced::widget::image::{FilterMethod, Image};
use iced::{Element, Event, Length, Rectangle, Rotation, Size};
use iced_wgpu::primitive::Renderer as PrimitiveRenderer;
use std::marker::PhantomData;

/// Active widget that renders one [`CastHandle`].
pub struct CastView<'a, Message, S = ManualSource, Theme = iced::Theme, Renderer = iced::Renderer>
where
    S: Source,
    Renderer: advanced::Renderer + PrimitiveRenderer,
{
    /// Handle rendered by the view and queried for redraw/error state.
    handle: CastHandle<S>,
    /// Wrapped stock image widget used for layout and draw-parameter shaping.
    image: Image<CastHandle<S>>,
    /// Optional mapper used to publish one message when a new source error appears.
    ///
    /// Returning `None` lets callers acknowledge an error without forcing a
    /// placeholder application message.
    on_error: Option<Box<dyn Fn(&S::Error) -> Option<Message> + 'a>>,
    /// Marker tying the widget to the app's message and renderer types.
    _marker: PhantomData<(Theme, Renderer)>,
}

impl<'a, Message, S, Theme, Renderer> CastView<'a, Message, S, Theme, Renderer>
where
    S: Source,
    Renderer: advanced::Renderer + PrimitiveRenderer,
{
    /// Builds one view for anything that exposes one shared [`CastHandle`].
    pub fn new(handle: impl AsRef<CastHandle<S>>) -> Self {
        let handle = handle.as_ref().clone();

        Self {
            image: Image::new(handle.clone()),
            handle,
            on_error: None,
            _marker: PhantomData,
        }
    }

    /// Sets the view width.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.image = self.image.width(width);
        self
    }

    /// Sets the view height.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.image = self.image.height(height);
        self
    }

    /// Sets whether the view should expand like `iced::widget::Image`.
    pub fn expand(mut self, expand: bool) -> Self {
        self.image = self.image.expand(expand);
        self
    }

    /// Sets the [`iced::ContentFit`] of the view.
    pub fn content_fit(mut self, content_fit: iced::ContentFit) -> Self {
        self.image = self.image.content_fit(content_fit);
        self
    }

    /// Sets the [`FilterMethod`] of the view.
    pub fn filter_method(mut self, filter_method: FilterMethod) -> Self {
        self.image = self.image.filter_method(filter_method);
        self
    }

    /// Applies the given [`Rotation`] to the view.
    pub fn rotation(mut self, rotation: impl Into<Rotation>) -> Self {
        self.image = self.image.rotation(rotation);
        self
    }

    /// Sets the opacity of the view.
    pub fn opacity(mut self, opacity: impl Into<f32>) -> Self {
        self.image = self.image.opacity(opacity);
        self
    }

    /// Sets the scale of the view.
    pub fn scale(mut self, scale: impl Into<f32>) -> Self {
        self.image = self.image.scale(scale);
        self
    }

    /// Crops the view to the given region in source pixel coordinates.
    pub fn crop(mut self, region: Rectangle<u32>) -> Self {
        self.image = self.image.crop(region);
        self
    }

    /// Sets the [`border::Radius`] of the view.
    pub fn border_radius(mut self, border_radius: impl Into<border::Radius>) -> Self {
        self.image = self.image.border_radius(border_radius);
        self
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
    Renderer: advanced::Renderer + PrimitiveRenderer,
{
    #[inline]
    fn size(&self) -> Size<Length> {
        <Image<CastHandle<S>> as Widget<Message, Theme, LayoutImageRenderer<S>>>::size(&self.image)
    }

    #[inline]
    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let renderer = LayoutImageRenderer::new(());

        <Image<CastHandle<S>> as Widget<Message, Theme, LayoutImageRenderer<S>>>::layout(
            &mut self.image,
            tree,
            &renderer,
            limits,
        )
    }

    #[inline]
    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &advanced::renderer::Style,
        layout: advanced::Layout<'_>,
        cursor: advanced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let mut draw_renderer =
            DrawImageRenderer::<S, _>::new(PrimitiveLiveRasterRenderer::new(renderer));

        <Image<CastHandle<S>> as Widget<Message, Theme, DrawImageRenderer<S, _>>>::draw(
            &self.image,
            tree,
            &mut draw_renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn update(
        &mut self,
        _tree: &mut widget::Tree,
        event: &Event,
        _layout: advanced::Layout<'_>,
        _cursor: advanced::mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn advanced::Clipboard,
        shell: &mut advanced::Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if let Some(on_error) = self.on_error.as_ref() {
            if let Some(error) = self.handle.take_last_error() {
                if let Some(message) = on_error(&error) {
                    shell.publish(message);
                }
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
    Renderer: 'a + advanced::Renderer + PrimitiveRenderer,
{
    fn from(widget: CastView<'a, Message, S, Theme, Renderer>) -> Self {
        Self::new(widget)
    }
}

struct XImageRenderer<S: Source, X> {
    /// Marker tying this renderer to the source type stored in the handle.
    _marker: PhantomData<fn() -> S>,
    x: X,
}
impl<S: Source, X> XImageRenderer<S, X> {
    #[inline]
    const fn new(x: X) -> Self {
        Self {
            _marker: PhantomData,
            x,
        }
    }
}

impl<S: Source, X> advanced::Renderer for XImageRenderer<S, X> {
    fn start_layer(&mut self, _bounds: Rectangle) {
        unexpected_image_renderer_method("start_layer")
    }

    fn end_layer(&mut self) {
        unexpected_image_renderer_method("end_layer")
    }

    fn start_transformation(&mut self, _transformation: iced::Transformation) {
        unexpected_image_renderer_method("start_transformation")
    }

    fn end_transformation(&mut self) {
        unexpected_image_renderer_method("end_transformation")
    }

    fn fill_quad(
        &mut self,
        _quad: advanced::renderer::Quad,
        _background: impl Into<iced::Background>,
    ) {
        unexpected_image_renderer_method("fill_quad")
    }

    fn reset(&mut self, _new_bounds: Rectangle) {
        unexpected_image_renderer_method("reset")
    }

    fn allocate_image(
        &mut self,
        _handle: &core_image::Handle,
        _callback: impl FnOnce(Result<core_image::Allocation, core_image::Error>) + Send + 'static,
    ) {
        unexpected_image_renderer_method("allocate_image")
    }
}

/// Renderer adapter used only for stock image layout.
type LayoutImageRenderer<S> = XImageRenderer<S, ()>;

impl<S: Source> core_image::Renderer for LayoutImageRenderer<S> {
    type Handle = CastHandle<S>;

    #[cold]
    fn load_image(
        &self,
        _handle: &Self::Handle,
    ) -> Result<core_image::Allocation, core_image::Error> {
        unexpected_image_renderer_method("load_image")
    }

    #[inline]
    fn measure_image(&self, handle: &Self::Handle) -> Option<Size<u32>> {
        handle
            .dimensions()
            .map(|(width, height)| Size::new(width, height))
    }

    #[cold]
    fn draw_image(
        &mut self,
        _image: core_image::Image<Self::Handle>,
        _bounds: Rectangle,
        _clip_bounds: Rectangle,
    ) {
        unexpected_image_renderer_method("draw_image")
    }
}

/// Renderer adapter used only for stock image draw.
type DrawImageRenderer<S, R> = XImageRenderer<S, R>;

impl<S: Source, Renderer: LiveRasterRenderer> core_image::Renderer
    for DrawImageRenderer<S, Renderer>
{
    type Handle = CastHandle<S>;

    #[cold]
    fn load_image(
        &self,
        _handle: &Self::Handle,
    ) -> Result<core_image::Allocation, core_image::Error> {
        unreachable!("draw image renderer never loads stock image allocations")
    }

    #[inline]
    fn measure_image(&self, handle: &Self::Handle) -> Option<Size<u32>> {
        LayoutImageRenderer::measure_image(&LayoutImageRenderer::new(()), handle)
    }

    #[inline]
    fn draw_image(
        &mut self,
        image: core_image::Image<Self::Handle>,
        bounds: Rectangle,
        clip_bounds: Rectangle,
    ) {
        let Some(image) = LiveImage::from_draw_request(image) else {
            return;
        };

        self.x.draw_live_image(image, bounds, clip_bounds);
    }
}

/// Panics when stock image layout or draw starts using one renderer method we do not expect.
#[cold]
fn unexpected_image_renderer_method(method: &str) -> ! {
    print!("Unhandled method `{}`", method);
    unreachable!("iced image unexpectedly called renderer method`")
}

#[cfg(test)]
mod tests {
    use super::LayoutImageRenderer;
    use crate::handle::CastHandle;
    use iced::advanced::layout;
    use iced::widget::image::Image;
    use iced::{ContentFit, Length, Rotation, Size};

    /// The wrapped stock image widget should drive live-cast layout exactly.
    #[test]
    fn stock_image_layout_drives_cast_view_layout() {
        let limits = layout::Limits::new(Size::ZERO, Size::new(400.0, 300.0));
        let handle = CastHandle::new();
        handle.present(
            crate::Frame::from_bgra(1920, 1080, vec![0; 1920usize * 1080usize * 4usize])
                .expect("frame should validate"),
        );
        let mut image = Image::new(handle)
            .width(Length::Shrink)
            .height(Length::Shrink)
            .content_fit(ContentFit::Contain)
            .rotation(Rotation::default());
        let mut tree = iced::advanced::widget::Tree::empty();
        let renderer = LayoutImageRenderer::<crate::ManualSource>::new(());

        let node = <Image<CastHandle> as iced::advanced::Widget<
            (),
            iced::Theme,
            LayoutImageRenderer<crate::ManualSource>,
        >>::layout(&mut image, &mut tree, &renderer, &limits);

        assert_eq!(node.size(), Size::new(400.0, 225.0));
    }
}
