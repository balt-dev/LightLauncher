use async_compat::Compat;
use crossbeam_queue::SegQueue;
use futures::stream::FuturesUnordered;
use indexmap::IndexMap;
use reqwest::{
    Method, StatusCode, Url,
};
use smol::{Executor, Task, lock::OnceCell};
use std::{
    panic::catch_unwind,
    sync::{
        LazyLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::{
    models::{
        Account, AuthRequest, FetchStatus, RefreshRequest, RefreshResponse, ServerDisplay,
        ServerEntry, ServerInfo, ServerStatus,
    }, utils::{self, SerdeNever, request_bytes, request_json},
};

pub(crate) static EXECUTOR_THREADS: AtomicUsize = AtomicUsize::new(0);
static EXECUTOR: OnceCell<Executor> = OnceCell::new();
static TASKS: LazyLock<futures::stream::FuturesUnordered<Task<()>>> =
    LazyLock::new(|| FuturesUnordered::new());
static RESPONSES: SegQueue<WorkerResponse> = SegQueue::new();

fn exec() -> &'static Executor<'static> {
    EXECUTOR.get_or_init_blocking(|| {
        let worker_count = EXECUTOR_THREADS.load(Ordering::Relaxed);
        if worker_count == 0 {
            panic!("tried to load executor before executor thread count was set");
        }
        for n in 1..=worker_count {
            std::thread::Builder::new()
                .name(format!("worker-{}", n))
                .spawn(|| {
                    loop {
                        catch_unwind(|| smol::block_on(exec().run(smol::future::pending::<()>())))
                            .ok();
                    }
                })
                .expect("could not spawn executor thread");
        }
        let ex = Executor::new();
        ex.spawn(smol::process::driver()).detach();
        ex
    })
}

pub fn send(message: WorkerMessage) {
    TASKS.push(exec().spawn(Compat::new(async move {
        match message.handle().await {
            Ok(response) => {
                log::debug!("[worker] responding: {response:?}");
                RESPONSES.push(response);
            }
            Err(err) => log::error!("[worker] error while handling message: {err:?}"),
        }
    })));
}

pub fn check_for_response() -> Option<WorkerResponse> {
    RESPONSES.pop()
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
    FetchServers {
        hub: String,
    },
    FetchServerData {
        url: String,
    },
    FetchServerInfo {
        url: String,
    },
    SynchronizeSigningKey {
        url: String,
    },
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
    FetchFailure {
        error: anyhow::Error,
    },
    FetchSuccessful {
        servers: IndexMap<String, ServerDisplay>,
    },
    StatusFetchSuccessful {
        url: String,
        status: ServerStatus,
    },
    StatusFetchFailure {
        url: String,
        error: anyhow::Error,
    },
    InfoFetchSuccessful {
        url: String,
        info: ServerInfo,
    },
    InfoFetchFailure {
        url: String,
        error: anyhow::Error,
    },
    SyncKeySuccess {
        key: String,
    },
    SyncKeyFailure {
        error: anyhow::Error,
    },
}

impl WorkerMessage {
    async fn handle(self) -> Result<WorkerResponse, anyhow::Error> {
        match self {
            WorkerMessage::Ping => Ok(WorkerResponse::Pong),
            WorkerMessage::LogIn { auth, url } => match request_json::<Account, AuthRequest>(
                url.join("authenticate")?,
                Some(auth),
                Method::POST,
            )
            .await
            {
                Ok(account) => Ok(WorkerResponse::LogInSuccessful { account }),
                Err(err) => Ok(WorkerResponse::LogInFailure { error: err.into() }),
            },
            WorkerMessage::RefreshToken { id, auth, url } => {
                match request_json::<RefreshResponse, RefreshRequest>(
                    url.join("refresh")?,
                    Some(auth),
                    Method::POST,
                )
                .await
                {
                    Ok(refresh) => Ok(WorkerResponse::RefreshSuccessful { id, refresh }),
                    Err(err) if let Some(StatusCode::UNAUTHORIZED) = err.status() => {
                        Ok(WorkerResponse::RefreshExpired { id })
                    }
                    Err(err) => Ok(WorkerResponse::RefreshFailure {
                        id,
                        error: err.into(),
                    }),
                }
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
                let servers = match request_json::<Vec<ServerEntry>, SerdeNever>(
                    Url::try_from(&*hub)?,
                    None,
                    Method::GET,
                )
                .await
                {
                    Ok(servers) => servers,
                    Err(err) => return Ok(WorkerResponse::FetchFailure { error: err.into() }),
                };
                let mut servers = servers
                    .into_iter()
                    .map(|server| ServerDisplay {
                        listing: server.listing,
                        status: crate::models::FetchStatus::Fetched {
                            status: server.status_data,
                            info: crate::models::InfoStatus::Unfetched,
                        },
                    })
                    .collect::<Vec<_>>();
                servers.sort_unstable_by_key(|s| match &s.status {
                    FetchStatus::Fetched { status, .. } => u32::MAX - status.players,
                    _ => u32::MAX,
                });
                let servers = servers
                    .into_iter()
                    .map(|s| (s.listing.address.clone(), s))
                    .collect();
                Ok(WorkerResponse::FetchSuccessful { servers })
            }
            WorkerMessage::FetchServerData { url } => {
                match request_json::<ServerStatus, SerdeNever>(
                    Url::try_from(&*(utils::sanitize_url(&url) + "/status"))?,
                    None,
                    Method::GET,
                )
                .await
                {
                    Ok(status) => Ok(WorkerResponse::StatusFetchSuccessful { url, status }),
                    Err(err) if err.is_builder() => Ok(WorkerResponse::StatusFetchFailure {
                        url,
                        error: anyhow::anyhow!("Invalid URL"),
                    }),
                    Err(err) if err.is_timeout() => Ok(WorkerResponse::StatusFetchFailure {
                        url,
                        error: anyhow::anyhow!("Request timed out"),
                    }),
                    Err(err) => Ok(WorkerResponse::StatusFetchFailure {
                        url,
                        error: err.into(),
                    }),
                }
            }
            WorkerMessage::FetchServerInfo { url } => match request_json::<ServerInfo, SerdeNever>(
                Url::try_from(&*(utils::sanitize_url(&url) + "/info"))?,
                None,
                Method::GET,
            )
            .await
            {
                Ok(info) => Ok(WorkerResponse::InfoFetchSuccessful { url, info }),
                Err(err) if err.is_builder() => Ok(WorkerResponse::InfoFetchFailure {
                    url,
                    error: anyhow::anyhow!("Invalid URL"),
                }),
                Err(err) if err.is_timeout() => Ok(WorkerResponse::InfoFetchFailure {
                    url,
                    error: anyhow::anyhow!("Request timed out"),
                }),
                Err(err) => Ok(WorkerResponse::InfoFetchFailure {
                    url,
                    error: err.into(),
                }),
            },
            WorkerMessage::SynchronizeSigningKey { url } => match request_bytes::<SerdeNever>(
                Url::try_from(&*utils::sanitize_url(&url))?,
                None,
                Method::GET,
            )
                .await
                .map_err(|err| anyhow::anyhow!(err))
                .and_then(|key| String::from_utf8(key.to_vec()).map_err(|err| anyhow::anyhow!(err)))
            {
                Ok(key) => Ok(WorkerResponse::SyncKeySuccess { key }),
                Err(error) => Ok(WorkerResponse::SyncKeyFailure {
                    error: error.into(),
                }),
            },
        }
    }
}
