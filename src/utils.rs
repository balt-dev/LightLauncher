use std::{
    fmt::Display, fs::{self, File}, sync::{LazyLock, Mutex, MutexGuard}, thread, time::Duration,
};

use bytes::Bytes;
use chrono::Local;
use deboa::{errors::{DeboaError, RequestError, ResponseError}, request::{DeboaRequest, DeboaRequestBuilder}, response::DeboaResponse};
use http::{Method, header};
use log::LevelFilter;
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, SharedLogger, TermLogger, TerminalMode, WriteLogger,
};
use smol::stream::StreamExt;
use deboa_smol::{Client, HttpBody};
use smol_timeout::TimeoutExt;
use url::Url;

use crate::{staticdata, worker::DownloadProgress};

pub static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| Client::default());

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

pub async fn request<U: serde::Serialize>(
    url: Url,
    method: Method,
    payload: Option<U>,
) -> Result<DeboaResponse, DeboaError> {
    let payload = match payload {
        Some(val) => {
            let body = serde_json::ser::to_string(&val)
                .map_err(|err| DeboaError::Request(RequestError::Parse { message: err.to_string() }))?;
            Some(Bytes::from(body.into_bytes()))
        },
        None => None
    };
    request_bytes(url, method, payload).await
}

pub async fn request_bytes(
    url: Url,
    method: Method,
    payload: Option<Bytes>,
) -> Result<DeboaResponse, DeboaError> {
    let mut req = deboa::request::get(url)?
        .method(method);
    if let Some(body) = payload {
        req = req.body(HttpBody::bytes(&body));
    }
    request_raw(req).await
}

pub async fn request_raw(req: DeboaRequestBuilder) -> Result<DeboaResponse, DeboaError> {
    let res = req
        .header(header::USER_AGENT, staticdata::USER_AGENT)?
        .send_with(&*HTTP_CLIENT)
        .timeout(staticdata::REQUEST_TIMEOUT)
        .await;
    let Some(res) = res else {
        return Err(DeboaError::Connection(deboa::errors::ConnectionError::Timeout { message: "request timed out".to_string() }))
    };
    res.and_then(|r| {
        let status = r.status();
        if status.is_success() {
            return Ok(r);
        } else {
            return Err(DeboaError::Response(ResponseError::Receive {
                status_code: status,
                message: status.canonical_reason().unwrap_or("?").to_string()
            }))
        }
    })
}

pub async fn get_bytes(
    response: DeboaResponse,
    mut update: impl FnMut(Option<DownloadProgress>),
) -> anyhow::Result<Bytes> {
    let mut res = Vec::new();
    let total = response.content_length().map(|i| i as usize).ok();
    let mut stream = response.stream();
    let mut current = 0;
    update(Some(DownloadProgress { current, total }));
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let Some(chunk) = chunk.data_ref() else { continue };
        current += chunk.len();
        update(Some(DownloadProgress { current, total }));
        res.extend(chunk);
    }
    update(None);
    Ok(bytes::Bytes::from(res))
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
