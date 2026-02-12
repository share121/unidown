use crate::{
    JS_RT, Parser, TOKIO_RT,
    abort::AbortOnDrop,
    fd::{ProgressState, download_segment},
    ffmpeg::ffmpeg,
    sanitize::{self, sanitize},
};
use anyhow::Context as _;
use fast_down::utils::gen_unique_path;
use gpui::{
    AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, SharedString, Styled,
    Task, Timer, Window, div,
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
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tracing::{Instrument, info_span};

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
            let frame = Arc::new(AtomicU64::new(0));
            let merge_speed = Arc::new(AtomicU64::new(0));
            let is_finished = Arc::new(AtomicBool::new(false));
            let task_handle = {
                let is_finished = is_finished.clone();
                let state = state.clone();
                let title = title.clone();
                let frame = frame.clone();
                let merge_speed = merge_speed.clone();
                TOKIO_RT.spawn(async move {
                    let _guard = scopeguard::guard((), |_| {
                        is_finished.store(true, Ordering::Relaxed);
                    });
                    let video_path = download_segment(
                        video_url,
                        &title,
                        "mp4",
                        &output_dir,
                        &client,
                        &state,
                        4,
                        headers,
                    )
                    .await?;
                    let merge_filename = sanitize(format!(
                        "{}-转码.mp4",
                        sanitize::truncate_to_bytes(&title, 230)
                    ));
                    let merge_path = gen_unique_path(soft_canonicalize::soft_canonicalize(
                        output_dir.join(merge_filename),
                    )?)
                    .await?;
                    let span = info_span!("转码视频");
                    ffmpeg(
                        [
                            "-i",
                            &video_path.to_string_lossy(),
                            "-c:v",
                            "h264_mf",
                            "-threads",
                            "0",
                            "-c:a",
                            "aac",
                            "-y",
                            &merge_path.to_string_lossy(),
                        ],
                        move |info| {
                            frame.store(info.frame, Ordering::Relaxed);
                            merge_speed.store((info.speed * 1000.) as u64, Ordering::Relaxed);
                        },
                    )
                    .instrument(span)
                    .await?;
                    let _ = tokio::fs::remove_file(&video_path).await;
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
                    frame,
                    merge_speed,
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
    frame: Arc<AtomicU64>,
    merge_speed: Arc<AtomicU64>,
    is_finished: Arc<AtomicBool>,
    _guard: AbortOnDrop<anyhow::Result<()>>,
}

impl Render for DouyinView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (text, pct) = self.state.display();

        let frame = self.frame.load(Ordering::Relaxed);
        let merge_speed = self.merge_speed.load(Ordering::Relaxed) as f64 / 1000.;
        let merge_text = format!("frame: {} | speed: {:.2}x", frame, merge_speed);
        let done = self.is_finished.load(Ordering::Relaxed);

        v_flex()
            .p_4()
            .gap_4()
            .child(div().child(self.title.clone()).text_2xl().font_bold())
            .child(self.render_row("视频", text, pct))
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .child("视频转码 (速度取决于视频大小和电脑性能)")
                            .text_lg()
                            .font_bold(),
                    )
                    .child(merge_text),
            )
            .child(
                div()
                    .child(if done {
                        "全部完成，请检查桌面"
                    } else {
                        "下载还未完成，请耐心等待，点解析按钮可以打断下载并重试"
                    })
                    .text_2xl()
                    .font_bold(),
            )
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
