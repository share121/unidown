#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use color_eyre::eyre::eyre;
use gpui::{AppContext, Application};
use gpui_component::Root;
use tracing::level_filters::LevelFilter;
use tracing_error::ErrorLayer;
use tracing_subscriber::{
    EnvFilter, Registry,
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};
use unidown::{home::HomeView, window_options::window_options};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> color_eyre::Result<()> {
    Registry::default()
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with(ErrorLayer::default())
        .with(fmt::layer().pretty().with_span_events(FmtSpan::CLOSE))
        .init();
    color_eyre::install()?;

    let app = Application::new().with_assets(gpui_component_assets::Assets);
    app.run(move |cx| {
        gpui_component::init(cx);
        let options = window_options("Unidown 下载器".into(), 800., 600., cx);
        cx.spawn(async move |cx| {
            cx.open_window(options, |window, cx| {
                let view = cx.new(|cx| HomeView::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .map_err(|e| eyre!(e))?;
            Ok::<_, color_eyre::Report>(())
        })
        .detach();
    });
    Ok(())
}
