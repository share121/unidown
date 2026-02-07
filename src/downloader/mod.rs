use async_trait::async_trait;
use std::{any::Any, fmt::Debug, path::Path, sync::Arc};

pub mod bilibili;
mod udown;
pub use udown::*;

pub trait Context: Any + Send + Sync + Debug {}
impl<T: Any + Send + Sync + Debug> Context for T {}

#[derive(Clone, Debug)]
pub struct ResourceNode {
    pub title: String,
    pub selected: bool,
    pub tags: Vec<String>,
    pub asset_groups: Vec<AssetGroup>,
    pub children: Vec<ResourceNode>,
    /// 用来指导 asset_groups 的下载
    pub context: Arc<dyn Context>,
}

impl ResourceNode {
    pub fn get_context<T: 'static>(&self) -> Option<&T> {
        (self.context.as_ref() as &dyn Any).downcast_ref::<T>()
    }
}

#[derive(Clone, Debug)]
pub struct AssetGroup {
    pub title: String,
    pub variants: Vec<AssetVariant>,
}

#[derive(Clone, Debug)]
pub struct AssetVariant {
    pub id: u32,
    pub label: String,
    pub selected: bool,
}

#[async_trait]
pub trait Downloader: Send + Sync {
    fn name(&self) -> &'static str;
    async fn parse(&self, data: &str) -> color_eyre::Result<Vec<ResourceNode>>;
    async fn download(&self, nodes: &[ResourceNode], ouput: &Path) -> color_eyre::Result<()>;
}
