use std::sync::LazyLock;

use bytes::Bytes;
use futures::StreamExt;
use http::{Method, header};
use reqwest::{Client, RequestBuilder};
use url::Url;

use crate::{staticdata, worker::DownloadProgress};

pub static HTTP_CLIENT: LazyLock<&'static mut Client> = LazyLock::new(|| Box::leak(Box::new(Client::builder()
    .timeout(staticdata::REQUEST_TIMEOUT)
    .build().expect("failed to build client"))));

pub async fn request<U: serde::Serialize>(
    url: Url,
    method: Method,
    payload: Option<U>,
) -> reqwest::Result<reqwest::Response> {
    let payload = match payload {
        Some(val) => {
            let body = serde_json::ser::to_string(&val).expect("failed to encode serializable object");
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
) -> reqwest::Result<reqwest::Response> {
    let mut req = HTTP_CLIENT.request(method, url);
    if let Some(body) = payload {
        req = req.body(body);
    }
    request_raw(req).await
}

pub async fn request_raw(req: RequestBuilder) -> reqwest::Result<reqwest::Response> {
    let res = req
        .header(header::USER_AGENT, staticdata::USER_AGENT)
        .send()
        .await
        .and_then(|r| r.error_for_status());
    res
}

pub async fn get_bytes(
    response: reqwest::Response,
    mut update: impl FnMut(Option<DownloadProgress>),
) -> anyhow::Result<Bytes> {
    let mut res = Vec::new();
    let total = response.content_length().map(|i| i as usize);
    let mut stream = response.bytes_stream();
    let mut current = 0;
    update(Some(DownloadProgress { current, total }));
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        current += chunk.len();
        update(Some(DownloadProgress { current, total }));
        res.extend(chunk);
    }
    update(None);
    Ok(bytes::Bytes::from(res))
}