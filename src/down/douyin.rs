use crate::{
    JS_RT, Parser, TOKIO_RT,
    abort::AbortOnDrop,
    fd::{ProgressState, download_segment},
};
use anyhow::Context as _;
use gpui::{
    AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, SharedString, Styled,
    Task, Timer, Window, div, prelude::FluentBuilder,
};
use gpui_component::{StyledExt, h_flex, progress::Progress, v_flex};
use regex::Regex;
use reqwest::{
    Client,
    header::{self, HeaderMap},
};
use std::{
    env,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

fn get_headers(referer: &str) -> HeaderMap {
    HeaderMap::from_iter( [
        (header::REFERER, referer.parse().unwrap()),
        (header::ORIGIN, "https://www.douyin.com".parse().unwrap()),
        (header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36".parse().unwrap()),
    ])
}

#[derive(Default)]
pub struct DouyinDown {}

impl DouyinDown {
    pub fn new() -> Self {
        Self {}
    }
}

impl Parser for DouyinDown {
    fn parse(
        &self,
        input: &str,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Option<AnyView>>> {
        let client = Client::new();
        let client_cl = client.clone();
        let js_rt = JS_RT.clone();
        let headers = get_headers(input).into();
        let input = extract_modal_id(input)
            .map(|s| format!("https://www.douyin.com?{s}"))
            .unwrap_or_else(|| input.to_string());
        let fut = TOKIO_RT.spawn(async move { js_rt.parse_douyin(input, client_cl).await });
        cx.spawn(async move |cx| {
            let (title, video_url) = fut.await?.context("无法解析抖音视频链接")?;
            let title = SharedString::from(title);
            let output_dir = dirs::desktop_dir()
                .or_else(dirs::download_dir)
                .or_else(|| env::current_dir().ok())
                .context("找不到下载目录")?;
            let state = ProgressState::new();
            let is_finished = Arc::new(AtomicBool::new(false));
            let task_handle = {
                let is_finished = is_finished.clone();
                let state = state.clone();
                let title = title.clone();
                TOKIO_RT.spawn(async move {
                    let _guard = scopeguard::guard((), |_| {
                        is_finished.store(true, Ordering::Relaxed);
                    });
                    download_segment(
                        video_url,
                        &title,
                        "mp4",
                        &output_dir,
                        &client,
                        &state,
                        1,
                        headers,
                    )
                    .await?;
                    Ok(())
                })
            };
            let view = cx.new(|cx| {
                let finished_flag = is_finished.clone();
                cx.spawn(async move |view, cx| {
                    loop {
                        if finished_flag.load(Ordering::Relaxed) {
                            break;
                        }
                        Timer::after(Duration::from_millis(100)).await;
                        if view.update(cx, |_, cx| cx.notify()).is_err() {
                            break;
                        }
                    }
                })
                .detach();
                DouyinView {
                    title,
                    state,
                    is_finished,
                    _guard: AbortOnDrop(task_handle),
                }
            })?;
            Ok(Some(view.into()))
        })
    }
}

pub struct DouyinView {
    title: SharedString,
    state: Arc<ProgressState>,
    is_finished: Arc<AtomicBool>,
    _guard: AbortOnDrop<anyhow::Result<()>>,
}

impl Render for DouyinView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (text, pct) = self.state.display();
        let done = self.is_finished.load(Ordering::Relaxed);

        v_flex()
            .p_4()
            .gap_4()
            .child(div().child(self.title.clone()).text_2xl().font_bold())
            .child(self.render_row("视频", text, pct))
            .when(done, |this| {
                this.child(div().child("全部完成，请检查桌面").text_2xl().font_bold())
            })
    }
}

impl DouyinView {
    fn render_row(&self, label: &str, text: String, pct: f32) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .justify_between()
                    .child(div().child(label.to_string()).text_lg().font_bold())
                    .child(text),
            )
            .child(Progress::new().value(pct))
    }
}

fn extract_modal_id(url: &str) -> Option<&str> {
    lazy_static::lazy_static! {
        static ref MODAL_ID_REGEX: Regex = Regex::new(r"\bmodal_id=\d+?\b").unwrap();
    }
    MODAL_ID_REGEX
        .captures(url)
        .and_then(|c| c.get(0).map(|m| m.as_str()))
}
