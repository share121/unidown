use crate::decode::decode_text;
use std::{ffi::OsStr, fmt::Display};
use tokio::process::Command;

pub async fn ffmpeg<I, S>(args: I) -> Result<(), FFmpegError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("ffmpeg").args(args).output().await?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = decode_text(&output.stderr).trim().to_string();
    let stdout = decode_text(&output.stdout).trim().to_string();
    Err(FFmpegError::Runtime(FFmpegRuntimeError {
        code: output.status.code(),
        stderr,
        stdout,
    }))
}

#[derive(thiserror::Error, Debug)]
pub enum FFmpegError {
    #[error(transparent)]
    Init(#[from] std::io::Error),
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
