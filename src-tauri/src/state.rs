use std::path::PathBuf;
use std::sync::Mutex;

use crate::config::ConfigEngine;
use crate::launcher::Launcher;

pub struct Inner {
    pub data_dir: PathBuf,
    pub instances_dir: PathBuf,
    pub java_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub config: ConfigEngine,
    pub launcher: Mutex<Launcher>,
}

pub struct AppState {
    pub inner: Mutex<Inner>,
}
