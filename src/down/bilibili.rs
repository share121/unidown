use crate::{
    Parser, TOKIO_RT,
    fd::{ProgressInfo, fd},
    ffmpeg::ffmpeg,
    format_size,
    sanitize::sanitize,
};
use anyhow::{Context, anyhow, bail};
use fast_down::utils::gen_unique_path;
use gpui::{
    AnyView, App, AppContext, ParentElement, Render, SharedString, Styled, Task, Window, div,
    prelude::FluentBuilder,
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
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tracing::{Instrument, info_span};

lazy_static! {
    static ref DEFAULT_HEADERS: Arc<HeaderMap> = HeaderMap::from_iter([
        (header::ORIGIN, "https://www.bilibili.com".parse().unwrap()),
        (
            header::REFERER,
            "https://www.bilibili.com/".parse().unwrap()
        ),
        (
            header::USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36".parse().unwrap()
        ),
    ])
    .into();
}

pub struct BiliDown {
    client: Client,
}

impl Parser for BiliDown {
    fn parse(
        &self,
        input: &str,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Option<AnyView>>> {
        let bvid = extract_bvid(input).map(|s| s.to_string());
        let client = self.client.clone();
        cx.spawn(async move |cx| {
            let bvid = bvid.context("无法提取 bilibili 视频的 bvid")?;
            let ouput_dir = dirs::desktop_dir()
                .or_else(dirs::download_dir)
                .or_else(|| env::current_dir().ok())
                .context("无法找到输出路径")?;
            let client_clone = client.clone();
            let (title, (video_url, audio_url)) = TOKIO_RT
                .spawn(async move {
                    tokio::try_join!(
                        get_title(&bvid, &client_clone),
                        get_info(&bvid, &client_clone)
                    )
                })
                .await??;
            let video_speed = Arc::new(AtomicU64::new(0));
            let video_curr = Arc::new(AtomicU64::new(0));
            let video_total = Arc::new(AtomicU64::new(0));
            let video_speed_clone = video_speed.clone();
            let video_curr_clone = video_curr.clone();
            let video_total_clone = video_total.clone();
            let video_path = ouput_dir.join(sanitize(&format!("{}.mp4", title)));
            let video_client = client.clone();
            let video_fut = async move {
                let video_path = gen_unique_path(video_path).await?;
                fd(
                    video_url,
                    &video_path,
                    &video_client,
                    DEFAULT_HEADERS.clone(),
                    |info: ProgressInfo| {
                        video_speed_clone.store(info.speed_bps, Ordering::Relaxed);
                        video_curr_clone.store(info.downloaded, Ordering::Relaxed);
                        video_total_clone.store(info.total, Ordering::Relaxed);
                    },
                )
                .await?;
                Ok::<_, anyhow::Error>(video_path)
            };
            let audio_speed = Arc::new(AtomicU64::new(0));
            let audio_curr = Arc::new(AtomicU64::new(0));
            let audio_total = Arc::new(AtomicU64::new(0));
            let audio_speed_clone = audio_speed.clone();
            let audio_curr_clone = audio_curr.clone();
            let audio_total_clone = audio_total.clone();
            let audio_path = ouput_dir.join(sanitize(&format!("{}.mp3", title)));
            let audio_client = client.clone();
            let audio_fut = async move {
                let audio_path = gen_unique_path(audio_path).await?;
                fd(
                    audio_url,
                    &audio_path,
                    &audio_client,
                    DEFAULT_HEADERS.clone(),
                    |info: ProgressInfo| {
                        audio_speed_clone.store(info.speed_bps, Ordering::Relaxed);
                        audio_curr_clone.store(info.downloaded, Ordering::Relaxed);
                        audio_total_clone.store(info.total, Ordering::Relaxed);
                    },
                )
                .await?;
                Ok::<_, anyhow::Error>(audio_path)
            };
            let merge_path = ouput_dir.join(sanitize(&format!("{}-合并.mp4", title)));
            let frame = Arc::new(AtomicU64::new(0));
            let merge_speed = Arc::new(AtomicU64::new(0));
            let frame_clone = frame.clone();
            let merge_speed_clone = merge_speed.clone();
            let is_finished = Arc::new(AtomicBool::new(false));
            let is_finished_clone = is_finished.clone();
            let handle = cx.spawn(async move |_| {
                TOKIO_RT
                    .spawn(async move {
                        let (video_path, audio_path) = tokio::try_join!(video_fut, audio_fut)?;
                        let merge_path = gen_unique_path(merge_path).await?;
                        let span = info_span!("合并音视频");
                        ffmpeg(
                            [
                                "-i",
                                &video_path.to_string_lossy(),
                                "-i",
                                &audio_path.to_string_lossy(),
                                "-c",
                                "copy",
                                &merge_path.to_string_lossy(),
                            ],
                            |info| {
                                frame_clone.store(info.frame, Ordering::Relaxed);
                                merge_speed_clone
                                    .store((info.speed * 1000.) as u64, Ordering::Relaxed);
                            },
                        )
                        .instrument(span)
                        .await?;
                        Ok::<_, anyhow::Error>(())
                    })
                    .await??;
                is_finished_clone.store(true, Ordering::Relaxed);
                Ok::<_, anyhow::Error>(())
            });
            let view = cx.new(|_| BiliView {
                title,
                audio_speed,
                audio_curr,
                audio_total,
                video_speed,
                video_curr,
                video_total,
                frame,
                merge_speed,
                handle,
                is_finished,
            })?;
            Ok(Some(view.into()))
        })
    }
}

impl BiliDown {
    pub fn new() -> anyhow::Result<Self> {
        let client = ClientBuilder::new()
            .default_headers(DEFAULT_HEADERS.as_ref().clone())
            .build()?;
        Ok(Self { client })
    }
}

pub struct BiliView {
    title: SharedString,
    audio_speed: Arc<AtomicU64>,
    audio_curr: Arc<AtomicU64>,
    audio_total: Arc<AtomicU64>,
    video_speed: Arc<AtomicU64>,
    video_curr: Arc<AtomicU64>,
    video_total: Arc<AtomicU64>,
    frame: Arc<AtomicU64>,
    merge_speed: Arc<AtomicU64>,
    #[allow(unused)]
    handle: Task<anyhow::Result<()>>,
    is_finished: Arc<AtomicBool>,
}

impl Render for BiliView {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let video_curr = self.video_curr.load(Ordering::Relaxed);
        let video_total = self.video_total.load(Ordering::Relaxed);
        let video_speed = self.video_speed.load(Ordering::Relaxed);
        let video_progress = video_curr as f32 / video_total as f32 * 100.;
        let video_size_str = format!(
            "{} / {} | {:.2}% | {}/s",
            format_size(video_curr as f64),
            format_size(video_total as f64),
            video_progress,
            format_size(video_speed as f64)
        );

        let audio_curr = self.audio_curr.load(Ordering::Relaxed);
        let audio_total = self.audio_total.load(Ordering::Relaxed);
        let audio_speed = self.audio_speed.load(Ordering::Relaxed);
        let audio_progress = audio_curr as f32 / audio_total as f32 * 100.;
        let audio_size_str = format!(
            "{} / {} | {:.2}% | {}/s",
            format_size(audio_curr as f64),
            format_size(audio_total as f64),
            audio_progress,
            format_size(audio_speed as f64)
        );

        let frame = self.frame.load(Ordering::Relaxed);
        let merge_speed = self.merge_speed.load(Ordering::Relaxed) as f64 / 1000.;
        let merge_size_str = format!("frame: {} | speed: {:.2}x", frame, merge_speed);

        let is_finished = self.is_finished.load(Ordering::Relaxed);

        v_flex()
            .p_4()
            .gap_4()
            .child(div().child(self.title.clone()).text_2xl().font_bold())
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .justify_between()
                            .child(div().child("下载视频").text_lg().font_bold())
                            .child(video_size_str),
                    )
                    .child(Progress::new().value(video_progress)),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .justify_between()
                            .child(div().child("下载音频").text_lg().font_bold())
                            .child(audio_size_str),
                    )
                    .child(Progress::new().value(audio_progress)),
            )
            .child(
                h_flex()
                    .justify_between()
                    .child(div().child("合并音视频").text_lg().font_bold())
                    .child(merge_size_str),
            )
            .when(is_finished, |this| {
                this.child(
                    div()
                        .child("全部下载完成，请检查桌面")
                        .text_2xl()
                        .font_bold(),
                )
            })
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
