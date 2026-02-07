use crate::{Downloader, ResourceNode};
use async_trait::async_trait;
use std::{collections::HashMap, path::Path, sync::Arc};
use tokio::task::JoinSet;
use tracing::{error, instrument};

pub struct UDownloader {
    pub downloaders: HashMap<&'static str, Arc<dyn Downloader>>,
}

impl UDownloader {
    pub fn new(downloaders: &[Arc<dyn Downloader>]) -> Self {
        let mut res = HashMap::new();
        for downloader in downloaders {
            res.insert(downloader.name(), downloader.clone());
        }
        Self { downloaders: res }
    }
}

#[derive(Debug)]
struct UContext {
    name: &'static str,
}

#[async_trait]
impl Downloader for UDownloader {
    fn name(&self) -> &'static str {
        "unidown"
    }

    #[instrument(name = "unidown 解析器", skip(self))]
    async fn parse(&self, url: &str) -> color_eyre::Result<Vec<ResourceNode>> {
        let mut parsed = Vec::new();
        let mut task_set = JoinSet::new();
        let url: Arc<str> = url.into();
        for (&name, downloader) in &self.downloaders {
            let downloader = downloader.clone();
            let url = url.clone();
            task_set.spawn(async move { (name, downloader.parse(&url).await) });
        }
        while let Some(result) = task_set.join_next().await {
            let Ok((name, result)) = result else { continue };
            let Ok(children) = result else { continue };
            parsed.push(ResourceNode {
                title: name.to_string(),
                selected: false,
                tags: vec![],
                asset_groups: vec![],
                children,
                context: Arc::new(UContext { name }),
            });
        }
        Ok(parsed)
    }

    #[instrument(name = "unidown 下载器", skip(self, nodes))]
    async fn download(&self, nodes: &[ResourceNode], output: &Path) -> color_eyre::Result<()> {
        let mut task_set = JoinSet::new();
        let output: Arc<Path> = output.into();
        for node in nodes {
            let Some(ctx) = node.get_context::<UContext>() else {
                continue;
            };
            let Some(downloader) = self.downloaders.get(ctx.name).cloned() else {
                continue;
            };
            let output = output.clone();
            let children = node.children.clone();
            task_set.spawn(async move { downloader.download(&children, &output).await });
        }
        while let Some(result) = task_set.join_next().await {
            if let Err(e) = result {
                error!("下载线程意外退出：{}", e);
            } else if let Ok(Err(e)) = result {
                error!("下载失败：{}", e);
            }
        }
        Ok(())
    }
}
