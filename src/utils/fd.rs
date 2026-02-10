use fast_down::{
    Event, Merge, Total,
    file::MmapFilePusher,
    http::{HttpError, Prefetch},
    multi::{self, download_multi},
    utils::{FastDownPuller, FastDownPullerOptions},
};
use parking_lot::Mutex;
use reqwest::{Client, Url, header::HeaderMap};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Default)]
pub struct ProgressInfo {
    pub downloaded: u64,
    pub total: u64,
    /// 字节/秒
    pub speed_bps: u64,
}

pub async fn fd(
    url: Url,
    output: &Path,
    client: &Client,
    headers: Arc<HeaderMap>,
    on_progress: impl Fn(ProgressInfo) + Send + Sync,
) -> anyhow::Result<()> {
    let mut threads = 8;
    'retry: loop {
        info!("开始获取元数据");
        let (info, resp) = loop {
            match client.prefetch(url.clone()).await {
                Ok(t) => {
                    if t.0.fast_download {
                        break t;
                    }
                }
                Err((e, t)) => {
                    error!(err = ?e, "获取元数据失败");
                    tokio::time::sleep(t.unwrap_or(Duration::from_millis(500))).await;
                }
            }
        };
        info!(info = ?info, "已获取元数据");
        if info.size < 50 * 1024 * 1024 {
            info!("文件大小过小，不启用多线程下载");
            threads = 1;
        }
        let puller = FastDownPuller::new(FastDownPullerOptions {
            url: url.clone(),
            headers: headers.clone(),
            proxy: "",
            multiplexing: false,
            accept_invalid_certs: false,
            accept_invalid_hostnames: false,
            file_id: info.file_id,
            resp: Some(Arc::new(Mutex::new(Some(resp)))),
        })?;
        let total = info.size;
        let pusher = MmapFilePusher::new(&output, total).await?;
        let result = download_multi(
            puller,
            pusher,
            multi::DownloadOptions {
                #[allow(clippy::single_range_in_vec_init)]
                download_chunks: [0..total].iter(),
                retry_gap: Duration::from_millis(500),
                concurrent: threads,
                push_queue_cap: 1024,
                min_chunk_size: 8 * 1024,
            },
        );
        let mut progress = Vec::new();
        let mut smoothed_speed = 0.;
        let alpha = 0.3;
        let mut last_update = Instant::now();
        let mut last_bytes = 0;
        let start = last_update;
        let mut retry_count = 0;
        while let Ok(e) = result.event_chain.recv().await {
            match e {
                Event::FlushError(e) => error!("磁盘刷写失败: {:?}", e),
                Event::PullError(id, e) => {
                    warn!("下载数据出错 {}: {:?}", id, e);
                    if let HttpError::MismatchedBody(_) = e {
                        retry_count += 1;
                        if retry_count > threads * 2 {
                            threads = (threads / 2).max(1);
                            error!(threads = threads, "下载数据出错过多，完全重试");
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            continue 'retry;
                        }
                    }
                }
                Event::PushError(id, e) => error!("写入数据出错 {}: {:?}", id, e),
                Event::Pulling(_) => {}
                Event::PullProgress(_, _) => {}
                Event::Finished(_) => {}
                Event::PushProgress(_, p) => {
                    progress.merge_progress(p);
                    let now = Instant::now();
                    let elapsed = now - last_update;
                    let elapsed_secs = elapsed.as_secs_f64();
                    if elapsed_secs > 0.2 {
                        let downloaded = progress.total();
                        let bytes_diff = downloaded - last_bytes;
                        let instant_speed = bytes_diff as f64 / elapsed_secs;

                        smoothed_speed = if smoothed_speed == 0. {
                            instant_speed
                        } else {
                            alpha * instant_speed + (1.0 - alpha) * smoothed_speed
                        };

                        last_bytes = downloaded;
                        last_update = now;

                        let progress_info = ProgressInfo {
                            downloaded,
                            total,
                            speed_bps: smoothed_speed as u64,
                        };
                        on_progress(progress_info);
                    }
                }
            }
        }
        result.join().await?;
        let progress_info = ProgressInfo {
            downloaded: info.size,
            total,
            speed_bps: (info.size as f64 / start.elapsed().as_secs_f64()) as u64,
        };
        on_progress(progress_info);
        break Ok(());
    }
}
