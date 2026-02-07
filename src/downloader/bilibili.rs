use crate::{AssetGroup, AssetVariant, Downloader, ResourceNode, fd::fd, ffmpeg::ffmpeg};
use async_trait::async_trait;
use color_eyre::eyre::{Context, ContextCompat, bail, eyre};
use fast_down::utils::gen_unique_path;
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::{
    Client, ClientBuilder, Url,
    header::{self, HeaderMap},
};
use sanitize_filename::sanitize;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs;
use tracing::{Instrument, info_span, instrument};

const VIDEO_ID: u32 = 0;
const AUDIO_ID: u32 = 1;
const BOTH_ID: u32 = 2;

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

#[derive(Debug)]
pub struct BilibiliDownloader {
    client: Client,
}

#[derive(Debug)]
struct BilibiliContext {
    title: String,
    video_url: Url,
    audio_url: Url,
}

impl BilibiliDownloader {
    pub fn new() -> color_eyre::Result<Self> {
        let client = ClientBuilder::new()
            .default_headers(DEFAULT_HEADERS.as_ref().clone())
            .build()?;
        Ok(Self { client })
    }
}

#[async_trait]
impl Downloader for BilibiliDownloader {
    fn name(&self) -> &'static str {
        "bilibili"
    }

    #[instrument(name = "bilibili 解析器", skip(self))]
    async fn parse(&self, input: &str) -> color_eyre::Result<Vec<ResourceNode>> {
        let bvid = extract_bvid(input).wrap_err("无法提取 bilibili 视频的 bvid")?;
        let (title, (video_url, audio_url)) =
            tokio::try_join!(get_title(bvid, &self.client), get_info(bvid, &self.client))?;
        Ok(vec![ResourceNode {
            title: title.clone(),
            selected: true,
            tags: vec![],
            asset_groups: vec![AssetGroup {
                title: "下载内容".to_string(),
                variants: vec![
                    AssetVariant {
                        id: VIDEO_ID,
                        label: "视频".to_string(),
                        selected: false,
                    },
                    AssetVariant {
                        id: AUDIO_ID,
                        label: "音频".to_string(),
                        selected: false,
                    },
                    AssetVariant {
                        id: BOTH_ID,
                        label: "视频+音频合并".to_string(),
                        selected: true,
                    },
                ],
            }],
            children: vec![],
            context: Arc::new(BilibiliContext {
                title,
                video_url,
                audio_url,
            }),
        }])
    }

    #[instrument(name = "bilibili 下载器", skip(self, nodes))]
    async fn download(&self, nodes: &[ResourceNode], output: &Path) -> color_eyre::Result<()> {
        for node in nodes {
            let Some(ctx) = node.get_context::<BilibiliContext>() else {
                continue;
            };
            let features: Vec<_> = node.asset_groups[0]
                .variants
                .iter()
                .filter(|e| e.selected)
                .map(|e| e.id)
                .collect();
            let is_both = features.contains(&BOTH_ID);
            let is_video = features.contains(&VIDEO_ID);
            let is_audio = features.contains(&AUDIO_ID);
            let video_path = async {
                if is_both || is_video {
                    let span = info_span!("下载视频");
                    fd(
                        ctx.video_url.clone(),
                        &output.join(sanitize(format!("{}.mp4", ctx.title))),
                        &self.client,
                        DEFAULT_HEADERS.clone(),
                    )
                    .instrument(span)
                    .await
                } else {
                    Ok(PathBuf::new())
                }
            };
            let audio_path = async {
                if is_both || is_audio {
                    let span = info_span!("下载音频");
                    fd(
                        ctx.audio_url.clone(),
                        &output.join(sanitize(format!("{}.mp3", ctx.title))),
                        &self.client,
                        DEFAULT_HEADERS.clone(),
                    )
                    .instrument(span)
                    .await
                } else {
                    Ok(PathBuf::new())
                }
            };
            let (video_path, audio_path) = tokio::try_join!(video_path, audio_path)?;
            if is_both {
                let path =
                    gen_unique_path(output.join(sanitize(format!("{}-合并.mp4", ctx.title))))
                        .await?;
                let span = info_span!("合并音视频");
                ffmpeg([
                    "-i",
                    &video_path.to_string_lossy(),
                    "-i",
                    &audio_path.to_string_lossy(),
                    "-c",
                    "copy",
                    &path.to_string_lossy(),
                ])
                .instrument(span)
                .await?;
                if !is_video {
                    fs::remove_file(video_path).await?;
                }
                if !is_audio {
                    fs::remove_file(audio_path).await?;
                }
            }
        }
        Ok(())
    }
}

async fn get_cid(bvid: &str, client: &Client) -> color_eyre::Result<u64> {
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
        .ok_or_else(|| eyre!("bilibili API 错误: 没有 code"))?;
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
        .wrap_err("无法获取视频列表")?;
    let cid = first_page
        .get("cid")
        .and_then(|v| v.as_u64())
        .wrap_err("无法获取 cid")?;
    Ok(cid)
}

async fn get_title(bvid: &str, client: &Client) -> color_eyre::Result<String> {
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
        .ok_or_else(|| eyre!("bilibili API 错误: 没有 code"))?;
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
        .wrap_err("无法获取标题")?;
    Ok(title.to_string())
}

fn extract_bvid(url: &str) -> Option<&str> {
    lazy_static! {
        static ref BVID_REGEX: Regex = Regex::new(r"\bBV\w{10}\b").unwrap();
    }
    BVID_REGEX
        .captures(url)
        .and_then(|c| c.get(0).map(|m| m.as_str()))
}

async fn get_info(bvid: &str, client: &Client) -> color_eyre::Result<(Url, Url)> {
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
        .ok_or_else(|| eyre!("bilibili API 错误: 没有 code"))?;
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
        .wrap_err("无法获取 dash 数据")?;
    let video_url = dash
        .get("video")
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("baseUrl").or_else(|| v.get("base_url")))
        .and_then(|v| v.as_str())
        .wrap_err("无法获取视频")?
        .parse()
        .wrap_err("无法解析视频 URL")?;
    let audio_url = dash
        .get("audio")
        .and_then(|a| a.get(0))
        .and_then(|a| a.get("baseUrl").or_else(|| a.get("base_url")))
        .and_then(|a| a.as_str())
        .wrap_err("无法获取音频")?
        .parse()
        .wrap_err("无法解析音频 URL")?;
    Ok((video_url, audio_url))
}
