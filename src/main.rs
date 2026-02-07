use std::{path::Path, sync::Arc};
use tokio::fs;
use tracing::level_filters::LevelFilter;
use tracing_forest::ForestLayer;
use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt, util::SubscriberInitExt};
use udown::{Downloader, UDownloader, bilibili::BilibiliDownloader};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    Registry::default()
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with(ForestLayer::default())
        .init();

    let url = "https://www.bilibili.com/video/BV1NSrpBCEDm";
    let path = Path::new(".");
    let _ = fs::create_dir_all(&path).await;

    let downloaders: &[Arc<dyn Downloader>] = &[Arc::new(BilibiliDownloader::new()?)];
    let udownloader = UDownloader::new(downloaders);
    let res = udownloader.parse(url).await?;
    udownloader.download(&res, path).await?;

    Ok(())
}
