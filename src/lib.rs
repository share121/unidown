use crate::bilibili::BiliDown;
use std::sync::Arc;
use tokio::runtime::Runtime;

mod down;
mod utils;
mod view;

pub use down::*;
pub use utils::*;
pub use view::*;

lazy_static::lazy_static! {
    pub static ref TOKIO_RT: Runtime = {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    };
    pub static ref ALL_DOWN: AllDown = {
        let downs: &[Arc<dyn Down>] = &[
            Arc::new(BiliDown::new().expect("无法初始化 bilibili 解析器")),
        ];
        AllDown::new(downs)
    };
}
