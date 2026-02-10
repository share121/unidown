use crate::bilibili::BiliDown;
use gpui::{AnyView, Task, Window};
use tracing::error;

pub mod bilibili;

pub trait Parser: Send + Sync {
    fn parse(
        &self,
        input: &str,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> Task<anyhow::Result<Option<AnyView>>>;
}

pub struct AllDown {
    downs: Vec<Box<dyn Parser>>,
}

impl AllDown {
    pub fn new(downs: Vec<Box<dyn Parser>>) -> Self {
        Self { downs }
    }
}

impl Parser for AllDown {
    fn parse(
        &self,
        input: &str,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> Task<anyhow::Result<Option<AnyView>>> {
        let mut tasks = Vec::with_capacity(self.downs.len());
        for down in &self.downs {
            tasks.push(down.parse(input, window, cx));
        }
        cx.spawn(async move |_| {
            for task in tasks {
                match task.await {
                    Ok(Some(view)) => return Ok(Some(view)),
                    Ok(None) => {}
                    Err(e) => error!(err = ?e, "Error parsing input"),
                }
            }
            Ok(None)
        })
    }
}

lazy_static::lazy_static! {
    pub static ref ALL_DOWN: AllDown = {
        let downs: Vec<Box<dyn Parser>> = vec![
            Box::new(BiliDown::new().expect("无法初始化 bilibili 解析器")),
        ];
        AllDown::new(downs)
    };
}
