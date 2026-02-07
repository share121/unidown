use std::{path::Path, sync::Arc};
use tokio::fs;
use tracing::level_filters::LevelFilter;
use tracing_forest::ForestLayer;
use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt, util::SubscriberInitExt};
use unidown::{AllDown, Down, bilibili::BiliDown};

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

    let downs: &[Arc<dyn Down>] = &[Arc::new(BiliDown::new()?)];
    let alldown = AllDown::new(downs);

    let url = "https://www.bilibili.com/video/BV1NSrpBCEDm";
    let path = Path::new(".");
    let _ = fs::create_dir_all(&path).await;

    let res = alldown.parse(url).await?;
    alldown.download(&res, path).await?;

    Ok(())
}
