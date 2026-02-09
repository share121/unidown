use gpui::{App, Bounds, SharedString, WindowBounds, WindowOptions, px, size};
use gpui_component::TitleBar;

pub fn window_options(title: SharedString, w: f32, h: f32, cx: &App) -> WindowOptions {
    let bounds = Bounds::centered(None, size(px(w), px(h)), cx);
    let mut titlebar = TitleBar::title_bar_options();
    titlebar.title = Some(title);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(titlebar),
        ..Default::default()
    }
}
