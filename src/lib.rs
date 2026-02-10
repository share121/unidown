mod down;
mod fmt;
mod utils;
mod view;

pub use down::*;
pub use fmt::*;
pub use utils::*;
pub use view::*;

use std::path::PathBuf;
use tokio::runtime::Runtime;

lazy_static::lazy_static! {
    pub static ref TOKIO_RT: Runtime = {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    };
    pub static ref CURRENT_DIR: PathBuf = {
        let exe_path = std::env::current_exe().expect("无法获取当前可执行文件路径");
        exe_path.parent().expect("无法找到可执行文件目录").to_path_buf()
    };
    pub static ref FFMPEG_PATH: PathBuf = {
        let target_dir = CURRENT_DIR.as_path();
        let ffmpeg_name = if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        target_dir.join(ffmpeg_name)
    };
}
