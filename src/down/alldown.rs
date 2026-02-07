use crate::{Down, ResourceNode};
use async_trait::async_trait;
use std::{collections::HashMap, path::Path, sync::Arc};
use tokio::task::JoinSet;
use tracing::{error, instrument};

pub struct AllDown {
    pub downs: HashMap<&'static str, Arc<dyn Down>>,
}

impl AllDown {
    pub fn new(downs: &[Arc<dyn Down>]) -> Self {
        let mut res = HashMap::new();
        for down in downs {
            res.insert(down.name(), down.clone());
        }
        Self { downs: res }
    }
}

#[derive(Debug)]
struct AllCtx {
    name: &'static str,
}

#[async_trait]
impl Down for AllDown {
    fn name(&self) -> &'static str {
        "unidown"
    }

    #[instrument(name = "unidown 解析器", skip(self))]
    async fn parse(&self, url: &str) -> color_eyre::Result<Vec<ResourceNode>> {
        let mut parsed = Vec::new();
        let mut task_set = JoinSet::new();
        let url: Arc<str> = url.into();
        for (&name, down) in &self.downs {
            let down = down.clone();
            let url = url.clone();
            task_set.spawn(async move { (name, down.parse(&url).await) });
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
                context: Arc::new(AllCtx { name }),
            });
        }
        Ok(parsed)
    }

    #[instrument(name = "unidown 下载器", skip(self, nodes))]
    async fn download(&self, nodes: &[ResourceNode], output: &Path) -> color_eyre::Result<()> {
        let mut task_set = JoinSet::new();
        let output: Arc<Path> = output.into();
        for node in nodes {
            let Some(ctx) = node.get_context::<AllCtx>() else {
                continue;
            };
            let Some(down) = self.downs.get(ctx.name).cloned() else {
                continue;
            };
            let output = output.clone();
            let children = node.children.clone();
            task_set.spawn(async move { down.download(&children, &output).await });
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
