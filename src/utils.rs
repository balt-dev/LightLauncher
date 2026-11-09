use std::{
    fmt::Display, fs::{self, File}, io::Write, sync::{LazyLock, Mutex, MutexGuard},
};

use blake2::{Blake2b, Digest, digest::consts::U32};
use chrono::Local;
use log::LevelFilter;
use reqwest::{Client, Method, Url, header};
use simplelog::{
    ColorChoice, CombinedLogger, Config, ConfigBuilder, SharedLogger, TermLogger, TerminalMode,
    WriteLogger,
};

use crate::{staticdata, utils};

pub static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| Client::new());

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

async fn request_inner<U: serde::Serialize>(
    url: Url,
    auth: Option<U>,
    method: Method,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut req = HTTP_CLIENT
        .request(method, url)
        .header(header::USER_AGENT, staticdata::USER_AGENT)
        .timeout(staticdata::REQUEST_TIMEOUT);
    if let Some(auth) = auth {
        req = req.json(&auth);
    }
    let res = req.send().await;

    res.and_then(|r| r.error_for_status())
}

pub async fn request_bytes<U: serde::Serialize>(
    url: Url,
    auth: Option<U>,
    method: Method,
) -> Result<bytes::Bytes, reqwest::Error> {
    match request_inner(url, auth, method).await {
        Ok(r) => r.bytes().await,
        Err(err) => Err(err),
    }
}

pub async fn request_json<T: serde::de::DeserializeOwned, U: serde::Serialize>(
    url: Url,
    auth: Option<U>,
    method: Method,
) -> Result<T, reqwest::Error> {
    match request_inner(url, auth, method).await {
        Ok(r) => r.json::<T>().await,
        Err(err) => Err(err),
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