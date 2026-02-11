use crate::{
    Parser, TOKIO_RT,
    fd::{ProgressInfo, fd},
    format_size,
    sanitize::sanitize,
};
use anyhow::Context as _;
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

// 模拟 PC 端 Header，用于获取包含 RENDER_DATA 的 HTML
fn get_pc_headers() -> HeaderMap {
    HeaderMap::from_iter([
        (
            header::USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".parse().unwrap()
        ),
        (
            header::REFERER,
            "https://www.douyin.com/".parse().unwrap()
        ),
    ])
}

fn build_client() -> anyhow::Result<Client> {
    let client = ClientBuilder::new()
        .default_headers(get_pc_headers())
        // 必须开启 cookie 存储，否则容易被风控
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::default())
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
        let input_url = extract_url(input).map(|s| s.to_string());

        // 如果没有提取到 URL，直接返回（避免不必要的报错）
        // 如果是 Bilibili 链接，交给 Bilibili Parser 处理
        if input_url.is_none() {
            return Task::ready(Ok(None));
        }
        let input_url = input_url.unwrap();
        // 简单的预检查，如果不是抖音链接，直接忽略
        if !input_url.contains("douyin.com") {
            return Task::ready(Ok(None));
        }

        let client = build_client();
        cx.spawn(async move |cx| {
            let client = client.context("无法创建客户端")?;

            let output_dir = dirs::desktop_dir()
                .or_else(dirs::download_dir)
                .or_else(|| env::current_dir().ok())
                .context("找不到下载目录")?;

            let client_cl = client.clone();
            let (title, video_url) = TOKIO_RT
                .spawn(async move { get_douyin_info(&input_url, &client_cl).await })
                .await??;

            let progress_state = ProgressState::new();
            let is_finished = Arc::new(AtomicBool::new(false));

            let task_handle = {
                let is_finished = is_finished.clone();
                let progress_state = progress_state.clone();
                let title = title.clone();
                let client = client.clone();
                let output_dir = output_dir.clone();

                TOKIO_RT.spawn(async move {
                    let _guard = scopeguard::guard((), |_| {
                        is_finished.store(true, Ordering::Relaxed);
                    });

                    download_file(
                        video_url,
                        &title,
                        "mp4",
                        &output_dir,
                        &client,
                        &progress_state,
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

                let guard = Arc::new(AbortOnDrop(task_handle));
                DouyinView {
                    title,
                    state: progress_state,
                    is_finished,
                    _guard: guard,
                }
            })?;
            Ok(Some(view.into()))
        })
    }
}

async fn download_file(
    url: Url,
    title: &str,
    ext: &str,
    dir: &Path,
    client: &Client,
    state: &ProgressState,
) -> anyhow::Result<PathBuf> {
    let path = gen_unique_path(dir.join(sanitize(&format!("{}.{}", title, ext)))).await?;
    // 下载时使用 PC Header 也是安全的，或者不传特定 Header
    let headers = get_pc_headers();
    fd(url, &path, client, headers.into(), move |info| {
        state.update(info)
    })
    .await?;
    Ok(path)
}

pub struct DouyinView {
    title: SharedString,
    state: Arc<ProgressState>,
    is_finished: Arc<AtomicBool>,
    _guard: Arc<AbortOnDrop>,
}

impl Render for DouyinView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (text, pct) = self.state.display();
        let done = self.is_finished.load(Ordering::Relaxed);

        v_flex()
            .p_4()
            .gap_4()
            .child(div().child(self.title.clone()).text_2xl().font_bold())
            .child(self.render_row("下载进度", text, pct))
            .when(done, |this| {
                this.child(
                    div()
                        .child("下载完成")
                        .text_2xl()
                        .font_bold()
                        .text_color(gpui::white()),
                )
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

// --- 核心逻辑 ---

// 1. 增强 URL 提取，支持 ? = & % 等符号
fn extract_url(text: &str) -> Option<&str> {
    lazy_static! {
        static ref URL_REGEX: Regex = Regex::new(r"https?://[a-zA-Z0-9\./\-_?=&%]+").unwrap();
    }
    URL_REGEX.find(text).map(|m| m.as_str())
}

// 2. 增强 ID 提取，支持 modal_id
fn extract_douyin_id(url: &str) -> Option<String> {
    lazy_static! {
        static ref PATH_REGEX: Regex = Regex::new(r"(?:video|note)/(\d+)").unwrap();
        static ref QUERY_REGEX: Regex = Regex::new(r"modal_id=(\d+)").unwrap();
    }

    // 优先匹配 URL 参数 modal_id
    if let Some(c) = QUERY_REGEX.captures(url) {
        return Some(c.get(1)?.as_str().to_string());
    }
    // 其次匹配路径
    if let Some(c) = PATH_REGEX.captures(url) {
        return Some(c.get(1)?.as_str().to_string());
    }
    None
}

// 3. 核心：通过网页抓取获取数据，不依赖死掉的 API
async fn get_douyin_info(input_url: &str, client: &Client) -> anyhow::Result<(SharedString, Url)> {
    // 处理短链接
    let real_url = if input_url.contains("v.douyin.com") {
        client
            .get(input_url)
            .send()
            .await?
            .url()
            .as_str()
            .to_string()
    } else {
        input_url.to_string()
    };

    // 提取 ID
    let video_id =
        extract_douyin_id(&real_url).context(format!("无法从链接中解析出视频 ID: {}", real_url))?;

    // 构造标准 PC 视频页面 URL（无论原链接是 jingxuan 还是 note）
    let page_url = format!("https://www.douyin.com/video/{}", video_id);

    // 请求页面 HTML
    let resp = client.get(&page_url).send().await?;
    let html = resp.text().await?;

    // 提取 RENDER_DATA 中的 JSON
    // 抖音页面包含一个 id="RENDER_DATA" 的 script 标签，里面是 URL 编码的 JSON 数据
    let re =
        Regex::new(r#"<script id="RENDER_DATA" type="application/json">([^<]+)</script>"#).unwrap();
    let caps = re
        .captures(&html)
        .context("无法解析页面数据，可能触发了抖音风控 (Captcha)")?;
    let encoded_json = caps.get(1).unwrap().as_str();

    // 解码 URL 编码的内容
    // 使用 percent_encoding 库 (reqwest 的依赖)
    let decoded_json = percent_encoding::percent_decode_str(encoded_json).decode_utf8_lossy();

    // 解析 JSON
    let data: serde_json::Value =
        serde_json::from_str(&decoded_json).context("页面数据 JSON 解析失败")?;

    // 提取视频标题
    // 路径通常比较深，尝试多个可能的路径
    let title = data
        .pointer("/appContext/videoDetail/video/desc")
        .or_else(|| data.pointer("/appContext/videoDetail/item_list/0/desc")) // 有时在 item_list
        .and_then(|v| v.as_str())
        .unwrap_or("douyin_video");

    // 提取视频地址
    // 我们直接在 JSON 字符串中搜索 url_list，这样比解析多层 JSON 更稳健
    // 模式: "src":"//www.douyin.com/aweme/v1/play/..."
    // 或者: "playAddr":[{"src":"..."}]

    // 方法 A: JSON 路径提取 (优先)
    let src_opt = data
        .pointer("/appContext/videoDetail/video/playAddr/0/src")
        .or_else(|| data.pointer("/appContext/videoDetail/item_list/0/video/play_addr/url_list/0"))
        .and_then(|v| v.as_str());

    let mut video_url_str = if let Some(s) = src_opt {
        s.to_string()
    } else {
        // 方法 B: 正则兜底提取 (如果 JSON 结构变了)
        // 匹配包含 aweme/v1/play 的链接
        let re_fallback = Regex::new(r#"//[^\"]*?aweme/v1/play/[^\"]+"#).unwrap();
        re_fallback
            .find(&decoded_json)
            .map(|m| m.as_str().to_string())
            .context("无法找到视频下载地址")?
    };

    // 补全协议 (如果是 // 开头)
    if video_url_str.starts_with("//") {
        video_url_str = format!("https:{}", video_url_str);
    }

    // 尝试获取无水印高画质链接
    // 原始链接通常是 www.douyin.com/aweme/v1/play/...
    // 替换为 aweme.snssdk.com 并在 headers 中模拟移动端有时能拿到无水印
    // 但最简单的方法是解析出 video_id 参数
    if let Ok(url_obj) = Url::parse(&video_url_str) {
        if let Some((_, vid)) = url_obj.query_pairs().find(|(k, _)| k == "video_id") {
            // 构造官方无水印直链
            video_url_str = format!(
                "https://aweme.snssdk.com/aweme/v1/play/?video_id={}&ratio=720p&line=0",
                vid
            );
        }
    }

    Ok((SharedString::new(title.to_string()), video_url_str.parse()?))
}
