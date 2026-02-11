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

use crate::utils::js::JsRuntime;

lazy_static::lazy_static! {
    pub static ref TOKIO_RT: Runtime = {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    };
    pub static ref JS_RT: JsRuntime = JsRuntime::new();
    pub static ref FFMPEG_DIR: PathBuf = {
        dirs::home_dir().expect("Failed to get home directory").join(".unidown")
    };
    pub static ref FFMPEG_PATH: PathBuf = {
        let target_dir = FFMPEG_DIR.as_path();
        let ffmpeg_name = if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        target_dir.join(ffmpeg_name)
    };
}
