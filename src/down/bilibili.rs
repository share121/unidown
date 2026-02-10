use crate::{
    Parser, TOKIO_RT,
    fd::{ProgressInfo, fd},
    ffmpeg::ffmpeg,
    format_size,
    sanitize::sanitize,
};
use anyhow::{Context as _, anyhow, bail};
use fast_down::utils::gen_unique_path;
use gpui::{
    AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, SharedString, Styled,
    Task, Timer, Window, div, prelude::FluentBuilder,
};
use gpui_component::{StyledExt, h_flex, progress::Progress, v_flex};
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::{
    Client, ClientBuilder, Url,
    header::{self, HeaderMap},
};
use std::{
    env,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::task::JoinHandle;
use tracing::{Instrument, info_span};

fn get_headers(referer: &str) -> HeaderMap {
    HeaderMap::from_iter( [
        (header::REFERER, referer.parse().unwrap()),
        (header::ORIGIN, "https://www.bilibili.com".parse().unwrap()),
        (header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36".parse().unwrap()),
    ])
}

fn build_client(url: &str) -> anyhow::Result<Client> {
    let client = ClientBuilder::new()
        .default_headers(get_headers(url))
        .build()?;
    Ok(client)
}

struct AbortOnDrop(JoinHandle<anyhow::Result<()>>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Default)]
struct ProgressState {
    current: AtomicU64,
    total: AtomicU64,
    speed: AtomicU64,
}

impl ProgressState {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn update(&self, info: ProgressInfo) {
        self.speed.store(info.speed_bps, Ordering::Relaxed);
        self.current.store(info.downloaded, Ordering::Relaxed);
        self.total.store(info.total, Ordering::Relaxed);
    }

    fn display(&self) -> (String, f32) {
        let curr = self.current.load(Ordering::Relaxed) as f64;
        let total = self.total.load(Ordering::Relaxed) as f64;
        let speed = self.speed.load(Ordering::Relaxed) as f64;
        let pct = if total > 0.0 {
            (curr / total * 100.0) as f32
        } else {
            0.0
        };
        let text = format!(
            "{} / {} | {:.2}% | {}/s",
            format_size(curr),
            format_size(total),
            pct,
            format_size(speed)
        );
        (text, pct)
    }
}

#[derive(Default)]
pub struct BiliDown {}

impl BiliDown {
    pub fn new() -> Self {
        Self {}
    }
}

impl Parser for BiliDown {
    fn parse(
        &self,
        input: &str,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Option<AnyView>>> {
        let bvid = extract_bvid(input).map(|s| s.to_string());
        let client = build_client(input);
        cx.spawn(async move |cx| {
            let client = client.context("无法创建客户端")?;
            let bvid = bvid.context("无效的 BV 号")?;
            let output_dir = dirs::desktop_dir()
                .or_else(dirs::download_dir)
                .or_else(|| env::current_dir().ok())
                .context("找不到下载目录")?;
            let client_cl = client.clone();
            let (title, (video_url, audio_url)) = TOKIO_RT
                .spawn(async move {
                    tokio::try_join!(get_title(&bvid, &client_cl), get_info(&bvid, &client_cl))
                })
                .await??;
            let video_state = ProgressState::new();
            let audio_state = ProgressState::new();
            let frame = Arc::new(AtomicU64::new(0));
            let merge_speed = Arc::new(AtomicU64::new(0));
            let is_finished = Arc::new(AtomicBool::new(false));
            let task_handle = {
                let is_finished = is_finished.clone();
                let (video_state, audio_state, frame, merge_speed) = (
                    video_state.clone(),
                    audio_state.clone(),
                    frame.clone(),
                    merge_speed.clone(),
                );
                let (title, client, output_dir) =
                    (title.clone(), client.clone(), output_dir.clone());
                TOKIO_RT.spawn(async move {
                    let _guard = scopeguard::guard((), |_| {
                        is_finished.store(true, Ordering::Relaxed);
                    });
                    let (video_path, audio_path) = tokio::try_join!(
                        download_segment(
                            video_url,
                            &title,
                            "mp4",
                            &output_dir,
                            &client,
                            &video_state
                        ),
                        download_segment(
                            audio_url,
                            &title,
                            "mp3",
                            &output_dir,
                            &client,
                            &audio_state
                        )
                    )?;
                    let merge_path =
                        gen_unique_path(output_dir.join(sanitize(&format!("{}-合并.mp4", title))))
                            .await?;
                    let span = info_span!("合并音视频");
                    ffmpeg(
                        [
                            "-i",
                            &video_path.to_string_lossy(),
                            "-i",
                            &audio_path.to_string_lossy(),
                            "-c",
                            "copy",
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
                let guard = Arc::new(AbortOnDrop(task_handle));
                BiliView {
                    title,
                    video_state,
                    audio_state,
                    frame,
                    merge_speed,
                    is_finished,
                    _guard: guard,
                }
            })?;
            Ok(Some(view.into()))
        })
    }
}

async fn download_segment(
    url: Url,
    title: &str,
    ext: &str,
    dir: &Path,
    client: &Client,
    state: &ProgressState,
) -> anyhow::Result<PathBuf> {
    let path = gen_unique_path(dir.join(sanitize(&format!("{}.{}", title, ext)))).await?;
    let headers = get_headers(url.as_str()).into();
    fd(url, &path, client, headers, move |info| state.update(info)).await?;
    Ok(path)
}

pub struct BiliView {
    title: SharedString,
    video_state: Arc<ProgressState>,
    audio_state: Arc<ProgressState>,
    frame: Arc<AtomicU64>,
    merge_speed: Arc<AtomicU64>,
    is_finished: Arc<AtomicBool>,
    _guard: Arc<AbortOnDrop>,
}

impl Render for BiliView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (video_text, video_pct) = self.video_state.display();
        let (audio_text, audio_pct) = self.audio_state.display();

        let frame = self.frame.load(Ordering::Relaxed);
        let merge_speed = self.merge_speed.load(Ordering::Relaxed) as f64 / 1000.;
        let merge_text = format!("frame: {} | speed: {:.2}x", frame, merge_speed);
        let done = self.is_finished.load(Ordering::Relaxed);

        v_flex()
            .p_4()
            .gap_4()
            .child(div().child(self.title.clone()).text_2xl().font_bold())
            .child(self.render_row("视频", video_text, video_pct))
            .child(self.render_row("音频", audio_text, audio_pct))
            .child(
                h_flex()
                    .justify_between()
                    .child(div().child("合并处理").text_lg().font_bold())
                    .child(merge_text),
            )
            .when(done, |this| {
                this.child(div().child("全部完成，请检查桌面").text_2xl().font_bold())
            })
    }
}

impl BiliView {
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

async fn get_cid(bvid: &str, client: &Client) -> anyhow::Result<u64> {
    let body: serde_json::Value = client
        .get("https://api.bilibili.com/x/player/pagelist")
        .query(&[("bvid", bvid)])
        .send()
        .await?
        .json()
        .await?;
    let code = body
        .get("code")
        .and_then(|c| c.as_i64())
        .ok_or_else(|| anyhow!("bilibili API 错误: 没有 code"))?;
    if code != 0 {
        let msg = body
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("未知错误");
        bail!("bilibili API 错误: code: {}, message: {}", code, msg);
    }
    let first_page = body
        .get("data")
        .and_then(|data| data.get(0))
        .context("无法获取视频列表")?;
    let cid = first_page
        .get("cid")
        .and_then(|v| v.as_u64())
        .context("无法获取 cid")?;
    Ok(cid)
}

async fn get_title(bvid: &str, client: &Client) -> anyhow::Result<SharedString> {
    let body: serde_json::Value = client
        .get("https://api.bilibili.com/x/web-interface/view")
        .query(&[("bvid", bvid)])
        .send()
        .await?
        .json()
        .await?;
    let code = body
        .get("code")
        .and_then(|c| c.as_i64())
        .ok_or_else(|| anyhow!("bilibili API 错误: 没有 code"))?;
    if code != 0 {
        let msg = body
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("未知错误");
        bail!("bilibili API 错误: code: {}, message: {}", code, msg);
    }
    let title = body
        .get("data")
        .and_then(|data| data.get("title"))
        .and_then(|t| t.as_str())
        .context("无法获取标题")?;
    Ok(SharedString::new(title))
}

fn extract_bvid(url: &str) -> Option<&str> {
    lazy_static! {
        static ref BVID_REGEX: Regex = Regex::new(r"\bBV\w{10}\b").unwrap();
    }
    BVID_REGEX
        .captures(url)
        .and_then(|c| c.get(0).map(|m| m.as_str()))
}

async fn get_info(bvid: &str, client: &Client) -> anyhow::Result<(Url, Url)> {
    let cid = get_cid(bvid, client).await?;
    let body: serde_json::Value = client
        .get("https://api.bilibili.com/x/player/playurl?qn=80&fnval=4048&fourk=1&try_look=1")
        .query(&[("bvid", bvid), ("cid", &cid.to_string())])
        .send()
        .await?
        .json()
        .await?;
    let code = body
        .get("code")
        .and_then(|c| c.as_i64())
        .ok_or_else(|| anyhow!("bilibili API 错误: 没有 code"))?;
    if code != 0 {
        let msg = body
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("未知错误");
        bail!("bilibili API 错误: code: {}, message: {}", code, msg);
    }
    let dash = body
        .get("data")
        .and_then(|d| d.get("dash"))
        .context("无法获取 dash 数据")?;
    let video_url = dash
        .get("video")
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("baseUrl").or_else(|| v.get("base_url")))
        .and_then(|v| v.as_str())
        .context("无法获取视频")?
        .parse()
        .context("无法解析视频 URL")?;
    let audio_url = dash
        .get("audio")
        .and_then(|a| a.get(0))
        .and_then(|a| a.get("baseUrl").or_else(|| a.get("base_url")))
        .and_then(|a| a.as_str())
        .context("无法获取音频")?
        .parse()
        .context("无法解析音频 URL")?;
    Ok((video_url, audio_url))
}
