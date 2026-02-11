#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use anyhow::Context;
use gpui::{AppContext, Application};
use gpui_component::Root;
use sevenz_rust2::decompress;
use std::{io::Cursor, path::Path};
use tracing::{info, level_filters::LevelFilter};
use tracing_error::ErrorLayer;
use tracing_subscriber::{
    EnvFilter, Registry,
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};
use unidown::{FFMPEG_DIR, FFMPEG_PATH, home::HomeView, window_options::window_options};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    let has_console = unsafe {
        use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        AttachConsole(ATTACH_PARENT_PROCESS).is_ok()
    };
    if has_console {
        println!("检测到控制台启动");
    } else {
        println!("未检测到控制台");
    }

    Registry::default()
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with(fmt::layer().pretty().with_span_events(FmtSpan::CLOSE))
        .with(ErrorLayer::default())
        .init();
    install_ffmpeg()?;
    let app = Application::new().with_assets(gpui_component_assets::Assets);
    app.run(move |cx| {
        gpui_component::init(cx);
        let options = window_options("Unidown 下载器".into(), 800., 600., cx);
        cx.spawn(async move |cx| {
            cx.open_window(options, |window, cx| {
                let view = cx.new(|cx| HomeView::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
        })
        .detach();
    });
    Ok(())
}

fn install_ffmpeg() -> anyhow::Result<()> {
    if !FFMPEG_PATH.try_exists().unwrap_or(false) {
        info!("未检测到 ffmpeg，正在解压...");
        const FFMPEG_BYTES: &[u8] = include_bytes!("../ffmpeg.7z");
        let reader = Cursor::new(FFMPEG_BYTES);
        let ffmpeg_dir = FFMPEG_DIR.as_path();
        let _ = std::fs::create_dir_all(ffmpeg_dir);
        decompress(reader, FFMPEG_DIR.as_path()).context("解压 ffmpeg 失败")?;
        info!("解压完成");
    } else {
        info!("ffmpeg 已存在，跳过解压");
    }
    ensure_executable(&FFMPEG_PATH)?;
    Ok(())
}

/// 确保文件具有可执行权限（Unix 平台）
#[allow(unused_variables)]
fn ensure_executable(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        if perms.mode() & 0o111 == 0 {
            perms.set_mode(0o755);
            fs::set_permissions(path, perms)?;
        }
    }
    Ok(())
}
