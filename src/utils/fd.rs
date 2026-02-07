use color_eyre::eyre::eyre;
use fast_down::{
    Event,
    file::MmapFilePusher,
    http::Prefetch,
    multi::{self, download_multi},
    utils::{FastDownPuller, FastDownPullerOptions, gen_unique_path},
};
use parking_lot::Mutex;
use reqwest::{Client, Url, header::HeaderMap};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tracing::{error, warn};

pub async fn fd(
    url: Url,
    output: &Path,
    client: &Client,
    headers: Arc<HeaderMap>,
) -> color_eyre::Result<PathBuf> {
    let (info, resp) = client
        .prefetch(url.clone())
        .await
        .map_err(|(e, _)| eyre!("获取元数据失败: {:?}", e))?;
    let puller = FastDownPuller::new(FastDownPullerOptions {
        url: info.final_url,
        headers,
        proxy: "",
        multiplexing: false,
        accept_invalid_certs: false,
        accept_invalid_hostnames: false,
        file_id: info.file_id,
        resp: Some(Arc::new(Mutex::new(Some(resp)))),
    })?;
    let save_path = gen_unique_path(output).await?;
    let pusher = MmapFilePusher::new(&save_path, info.size).await?;
    let result = download_multi(
        puller,
        pusher,
        multi::DownloadOptions {
            #[allow(clippy::single_range_in_vec_init)]
            download_chunks: [0..info.size].iter(),
            retry_gap: Duration::from_millis(500),
            concurrent: 8,
            push_queue_cap: 1024,
            min_chunk_size: 8 * 1024,
        },
    );
    while let Ok(e) = result.event_chain.recv().await {
        match e {
            Event::FlushError(e) => error!("磁盘刷写失败: {:?}", e),
            Event::PullError(id, e) => warn!("下载数据出错 {}: {:?}", id, e),
            Event::PushError(id, e) => error!("写入数据出错 {}: {:?}", id, e),
            Event::Pulling(_) => {}
            Event::PullProgress(_, _) => {}
            Event::Finished(_) => {}
            Event::PushProgress(_, _) => {}
        }
    }
    result.join().await?;
    Ok(save_path)
}
