use crate::{FFMPEG_PATH, decode::decode_text};
use std::{ffi::OsStr, fmt::Display, process::Stdio};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

#[derive(Debug, Clone, Default)]
pub struct ProgressInfo {
    pub frame: u64,
    pub speed: f64,
}

pub async fn ffmpeg<I, S>(
    args: I,
    on_progress: impl Fn(ProgressInfo) + Send + Sync,
) -> Result<(), FFmpegError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(FFMPEG_PATH.as_os_str());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .args(args)
        .arg("-progress")
        .arg("pipe:1")
        .arg("-nostats")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout).lines();
    let mut progress = ProgressInfo::default();
    // FFmpeg 的进度输出格式为 key=value 换行，每一组进度以 progress=continue/end 结束
    while let Ok(Some(line)) = reader.next_line().await {
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "frame" => {
                    progress.frame = value.parse().unwrap_or(0);
                }
                "speed" => {
                    let speed_str = value.trim_end_matches('x');
                    progress.speed = speed_str.parse().unwrap_or(0.0);
                }
                "progress" => {
                    on_progress(progress.clone());
                    if value == "end" {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    let output = child.wait_with_output().await?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = decode_text(&output.stderr).trim().to_string();
        let stdout = decode_text(&output.stdout).trim().to_string();
        Err(FFmpegError::Runtime(FFmpegRuntimeError {
            code: output.status.code(),
            stderr,
            stdout,
        }))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum FFmpegError {
    #[error(transparent)]
    Std(#[from] std::io::Error),
    #[error(transparent)]
    Runtime(FFmpegRuntimeError),
}

#[derive(thiserror::Error, Debug)]
pub struct FFmpegRuntimeError {
    pub code: Option<i32>,
    pub stderr: String,
    pub stdout: String,
}

impl Display for FFmpegRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "FFmpeg 错误:")?;
        writeln!(f, "错误代码: {:?}", self.code)?;
        writeln!(f, "stderr: {}", self.stderr)?;
        writeln!(f, "stdout: {}", self.stdout)
    }
}
