use async_compat::Compat;
use ordered_hash_map::OrderedHashMap;
use reqwest::{
    Client, StatusCode,
    header::{self},
};
use std::{
    collections::HashMap, fmt::Display, sync::{Mutex, mpsc::{self, Receiver, RecvError, Sender, TryRecvError}}, thread::{self, JoinHandle}, time::Duration,
};

use crate::{
    models::{Account, AuthRequest, FetchStatus, RefreshRequest, RefreshResponse, ServerDisplay, ServerEntry, ServerInfo, ServerStatus}, staticdata::{self as s},
};

pub struct WorkerThread {
    send: Sender<WorkerMessage>,
    recv: Receiver<WorkerResponse>,
    handle: JoinHandle<Result<!, anyhow::Error>>,
}

#[derive(Default)]
pub struct WorkerContext {
    http: Client,
}

impl WorkerThread {
    pub fn unfuck(&mut self) -> Option<anyhow::Error> {
        if self.handle.is_finished() {
            let mut new = WorkerThread::init();
            std::mem::swap(self, &mut new);
            let Err(err) = new
                .handle
                .join()
                .map_err(|e| {
                    if let Some(str) = e.downcast_ref::<&'static str>() {
                        anyhow::anyhow!(&**str)
                    } else if let Some(str) = e.downcast_ref::<String>() {
                        anyhow::anyhow!(str.clone())
                    } else {
                        anyhow::anyhow!("unknown")
                    }
                })
                .flatten();
            Some(err)
        } else {
            None
        }
    }

    pub fn init() -> WorkerThread {
        let (main_send, worker_recv) = mpsc::channel::<WorkerMessage>();
        let (worker_send, main_recv) = mpsc::channel::<WorkerResponse>();
        let mut worker_ctx = Default::default();
        let handle = thread::spawn(move || -> Result<!, anyhow::Error> {
            loop {
                let msg = worker_recv.recv()?;
                log::debug!("[worker] recieved message: {msg:?}");
                match smol::block_on(Compat::new(msg.handle(&mut worker_ctx))) {
                    Ok(response) => {
                        log::debug!("[worker] responding: {response:?}");
                        worker_send.send(response)?;
                    }
                    Err(err) => log::error!("[worker] error while handling message: {err:?}"),
                }
            }
        });

        Self {
            handle,
            send: main_send,
            recv: main_recv,
        }
    }

    pub fn send(&self, message: WorkerMessage) -> Result<(), anyhow::Error> {
        match self.send.send(message) {
            Ok(()) => Ok(()),
            Err(err) => {
                log::warn!("failed to send message to worker thread, did it explode? (err: {err})");
                Err(err.into())
            }
        }
    }

    pub fn check_for_response(&self) -> Result<Option<WorkerResponse>, anyhow::Error> {
        match self.recv.try_recv() {
            Ok(val) => Ok(Some(val)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(err) => Err(anyhow::anyhow!(err))
        }
    }
}

#[derive(Debug)]
pub enum WorkerMessage {
    Ping,
    LogIn {
        auth: AuthRequest,
        url: reqwest::Url,
    },
    RefreshToken {
        id: String,
        auth: RefreshRequest,
        url: reqwest::Url,
    },
    OpenFileDialog {
        target: FileDialogTarget,
    },
    FetchServers { hub: String },
    FetchServerData { url: String },
    FetchServerInfo { url: String },
}

#[derive(Debug)]
pub enum FileDialogTarget {
    LoaderPath,
}

#[derive(Debug)]
pub enum WorkerResponse {
    Nop,
    Pong,
    RefreshSuccessful {
        id: String,
        refresh: RefreshResponse,
    },
    RefreshFailure {
        id: String,
        error: anyhow::Error,
    },
    RefreshExpired {
        id: String,
    },
    LogInSuccessful {
        account: Account,
    },
    LogInFailure {
        error: anyhow::Error,
    },
    FileChosen {
        path: String,
        target: FileDialogTarget,
    },
    FetchFailure { error: anyhow::Error },
    FetchSuccessful { servers: OrderedHashMap<String, ServerDisplay> },
    StatusFetchSuccessful { url: String, status: ServerStatus },
    StatusFetchFailure { url: String, error: anyhow::Error },
    InfoFetchSuccessful { url: String, info: ServerInfo },
    InfoFetchFailure { url: String, error: anyhow::Error },
}

impl WorkerMessage {
    async fn handle(self, ctx: &mut WorkerContext) -> Result<WorkerResponse, anyhow::Error> {
        match self {
            WorkerMessage::Ping => Ok(WorkerResponse::Pong),
            WorkerMessage::LogIn { auth, url } => {
                let res = ctx
                    .http
                    .post(url.join("authenticate")?)
                    .json(&auth)
                    .header(header::USER_AGENT, s::USER_AGENT)
                    .timeout(s::REQUEST_TIMEOUT)
                    .send()
                    .await;

                let response = match res.and_then(|r| r.error_for_status()) {
                    Ok(v) => v,
                    Err(err) => {
                        return Ok(WorkerResponse::LogInFailure { error: err.into() });
                    }
                };
                let account = match response.json::<Account>().await {
                    Ok(v) => v,
                    Err(err) => {
                        return Ok(WorkerResponse::LogInFailure { error: err.into() });
                    }
                };
                Ok(WorkerResponse::LogInSuccessful { account })
            }
            WorkerMessage::RefreshToken { id, auth, url } => {
                let res = ctx
                    .http
                    .post(url.join("refresh")?)
                    .json(&auth)
                    .header(header::USER_AGENT, s::USER_AGENT)
                    .timeout(s::REQUEST_TIMEOUT)
                    .send()
                    .await;

                let response = match res.and_then(|r| r.error_for_status()) {
                    Ok(v) => v,
                    Err(err) if err.status() == Some(StatusCode::UNAUTHORIZED) => {
                        return Ok(WorkerResponse::RefreshExpired { id });
                    }
                    Err(err) => {
                        return Ok(WorkerResponse::RefreshFailure {
                            id,
                            error: err.into(),
                        });
                    }
                };
                let refresh = match response.json::<RefreshResponse>().await {
                    Ok(v) => v,
                    Err(err) => {
                        return Ok(WorkerResponse::RefreshFailure {
                            id,
                            error: err.into(),
                        });
                    }
                };
                Ok(WorkerResponse::RefreshSuccessful { id, refresh })
            }
            WorkerMessage::OpenFileDialog { target } => {
                let dialog = rfd::AsyncFileDialog::new();
                let Some(file) = dialog.pick_file().await else {
                    return Ok(WorkerResponse::Pong);
                };
                Ok(WorkerResponse::FileChosen {
                    path: file.path().to_string_lossy().to_string(),
                    target,
                })
            }
            WorkerMessage::FetchServers { hub } => {
                let res = ctx
                    .http
                    .get(hub)
                    .header(header::USER_AGENT, s::USER_AGENT)
                    .timeout(s::REQUEST_TIMEOUT)
                    .send()
                    .await;

                let response = match res.and_then(|r| r.error_for_status()) {
                    Ok(v) => v,
                    Err(err) => {
                        return Ok(WorkerResponse::FetchFailure {
                            error: err.into(),
                        });
                    }
                };
                let server_list = match response.json::<Vec<ServerEntry>>().await {
                    Ok(v) => v,
                    Err(err) => {
                        return Ok(WorkerResponse::FetchFailure {
                            error: err.into(),
                        });
                    }
                };
                let mut servers = server_list.into_iter().map(|server|
                    ServerDisplay {
                        listing: server.listing,
                        status:  Mutex::new(crate::models::FetchStatus::Fetched {
                            status:server.status_data,
                            info: Mutex::new(crate::models::InfoStatus::Unfetched)
                        })
                    }
                ).collect::<Vec<_>>();
                servers.sort_by_key(|s| s.listing.display_name(match s.status.try_lock() {
                    Ok(lock) => match &*lock {
                        FetchStatus::Fetched { status, .. } => status.name.clone(),
                        _ => None
                    },
                    _ => None
                }).to_string());
                let servers = servers.into_iter().map(|s| (s.listing.address.clone(), s)).collect();
                Ok(WorkerResponse::FetchSuccessful { servers })
            }
            WorkerMessage::FetchServerData { url } => {
                let res = ctx
                    .http
                    .get(url.replace("ss14://", "http://").replace("ss14s://", "https://").trim_suffix('/').to_owned() + "/status")
                    .header(header::USER_AGENT, s::USER_AGENT)
                    .timeout(s::REQUEST_TIMEOUT)
                    .send()
                    .await;
                let response = match res.and_then(|r| r.error_for_status()) {
                    Ok(v) => v,
                    Err(error) => {
                        return Ok(WorkerResponse::StatusFetchFailure { url, error: anyhow::anyhow!(error) });
                    }
                };
                let status = match response.json::<ServerStatus>().await {
                    Ok(v) => v,
                    Err(error) => {
                        return Ok(WorkerResponse::StatusFetchFailure { url, error: anyhow::anyhow!(error) });
                    }
                };
                Ok(WorkerResponse::StatusFetchSuccessful { url, status })
            },
            WorkerMessage::FetchServerInfo { url } => {
                let res = ctx
                    .http
                    .get(url.replace("ss14://", "http://").replace("ss14s://", "https://").trim_suffix('/').to_owned() + "/info")
                    .header(header::USER_AGENT, s::USER_AGENT)
                    .timeout(s::REQUEST_TIMEOUT)
                    .send()
                    .await;
                let response = match res.and_then(|r| r.error_for_status()) {
                    Ok(v) => v,
                    Err(error) => {
                        return Ok(WorkerResponse::InfoFetchFailure { url, error: anyhow::anyhow!(error) });
                    }
                };
                let info = match response.json::<ServerInfo>().await {
                    Ok(v) => v,
                    Err(error) => {
                        return Ok(WorkerResponse::InfoFetchFailure { url, error: anyhow::anyhow!(error) });
                    }
                };
                Ok(WorkerResponse::InfoFetchSuccessful { url, info })
            },
        }
    }
}
