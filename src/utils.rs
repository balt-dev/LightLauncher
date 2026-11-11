use std::{
    fmt::Display, fs::{self, File}, sync::{LazyLock, Mutex, MutexGuard},
};

use bytes::Bytes;
use chrono::Local;
use futures::StreamExt;
use http::{Method, header};
use log::LevelFilter;
use reqwest::{Client, RequestBuilder};
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, SharedLogger, TermLogger, TerminalMode, WriteLogger,
};
use url::Url;

use crate::{staticdata, worker::DownloadProgress};

pub fn setup_logger() {
    let now = Local::now().format("%Y-%m-%dT%H-%M-%S");
    let conf = ConfigBuilder::new()
        .add_filter_allow_str("light_launcher")
        .set_time_format_rfc3339()
        .set_location_level(LevelFilter::Error)
        .set_target_level(LevelFilter::Error)
        .set_thread_level(LevelFilter::Error)
        .set_time_format_rfc3339()
        .build();
    let filter = cfg_select! {
        debug_assertions => LevelFilter::Debug,
        _ => LevelFilter::Info,
    };
    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        filter,
        conf.clone(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )];
    let log_path = staticdata::STATIC_DATA.datapath.join("logs");
    let res = fs::create_dir_all(&log_path)
        .and_then(|_| File::create(log_path.join(format!("{now}.log"))));
    match res {
        Ok(handle) => {
            loggers.push(WriteLogger::new(filter, conf, handle));
            CombinedLogger::init(loggers).expect("failed to initialize loggers :(");
        }
        Err(err) => {
            CombinedLogger::init(loggers).expect("failed to initialize loggers :(");
            log::error!("failed to open log file, logging only to stderr: {err}");
        }
    }
    log::info!("logging initialized");
}

pub trait MutexExt<T> {
    fn lock_unwrap(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_unwrap(&self) -> MutexGuard<'_, T> {
        self.lock().expect("other thread panicked")
    }
}

pub trait ResultExt {
    fn warn_for_error(&self);
}

impl<T, E: Display> ResultExt for Result<T, E> {
    fn warn_for_error(&self) {
        if let Err(err) = &self {
            log::warn!("Ignoring error: {err}");
        }
    }
}

pub enum SerdeNever {}

impl serde::Serialize for SerdeNever {
    fn serialize<S>(&self, _ser: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {}
    }
}

pub fn sanitize_url(url: &str) -> String {
    let mut url = url
        .replace("ss14://", "http://")
        .replace("ss14s://", "https://");
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        url = String::from("http://") + &*url;
    }
    url.trim_suffix('/').to_owned()
}
