use crossbeam_queue::SegQueue;
use deboa::errors::{DeboaError, ResponseError};
use deboa_smol::HttpBody;
use http::{HeaderName, Method, StatusCode};
use indexmap::{IndexMap, IndexSet};
use ruzstd::decoding::StreamingDecoder;
use smol::{Executor, lock::OnceCell};
use url::Url;
use zip::ZipArchive;
use std::{
    ffi::OsString, io::{Cursor, Read}, panic::catch_unwind, path::PathBuf, sync::{Arc, atomic::{AtomicUsize, Ordering}},
};

use crate::{
    core::Frontend, db::{DATABASE, Database}, gui::{HubStatus, Subwindow}, models::*, utils::{self, SerdeNever},
};

pub(crate) static EXECUTOR_THREADS: AtomicUsize = AtomicUsize::new(0);
static EXECUTOR: OnceCell<Executor> = OnceCell::new();
static MESSAGES: SegQueue<Box<dyn FnOnce(&mut Frontend<'_>) + Send>> = SegQueue::new();

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

pub enum FileDialogTarget {
    LoaderPath,
    ReplayPath,
}

pub fn handle(mut app: Frontend<'_>) {
    while let Some(message) = MESSAGES.pop() {
        message(&mut app);
    }
}

macro_rules! wrap_async_task {
    (
        payload = $payload_ty: ty;
        task = move || { $($task_block: tt)* };
        handle = move |$app: ident, $payload: ident| $handle_payload_block: tt;
    ) => { {
        let coro = async move {$($task_block)*};
        exec().spawn(async move {
            let res = coro.await;
            MESSAGES.push(Box::new(move |app| (|$app: &mut Frontend<'_>, $payload: $payload_ty| $handle_payload_block)(app, res)))
        }).detach();
    } };
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub enum DownloadId {
    ServerList,
    BuildManifestCount,
    BuildManifest,
    BuildZip
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct DownloadProgress {
    pub current: usize,
    pub total: Option<usize>,
}

pub(crate) fn login(auth: AuthRequest, url: Url) {
    wrap_async_task! {
        payload = anyhow::Result<Account>;
        task = move || {
            let response = utils::request(url, Method::POST, Some(auth)).await?;
            let data = utils::get_bytes(response, |_| {}).await?;
            let model = serde_json::from_slice(&data)?;
            Ok::<_, anyhow::Error>(model)
        };
        handle = move |app, res| {
            let Frontend::GUI(app) = app else { return };
            if let Some(Subwindow::AddAccount { message, .. }) = &mut app.subwindow {
                match res {
                    Ok(account) => {
                        app.subwindow = None;
                        app.config
                            .accounts
                            .insert(account.user_id.clone(), account);
                    },
                    Err(err) => {
                        *message = format!("Failed: {err}");
                    }
                }
            }
        };
    }
}

pub(crate) fn open_file_dialog(target: FileDialogTarget) {
    wrap_async_task! {
        payload = (FileDialogTarget, Option<PathBuf>);
        task = move || {
            let mut dialog = rfd::AsyncFileDialog::new();
            match target {
                FileDialogTarget::LoaderPath => {},
                FileDialogTarget::ReplayPath => dialog = dialog.add_filter(".zip files", &[".zip"]),
            }
            (target, dialog.pick_file().await.map(|p| p.path().to_path_buf()))
        };
        handle = move |app, res| {
            let Frontend::GUI(app) = app else { return };
            let (target, Some(path)) = res else { return };
            match target {
                FileDialogTarget::LoaderPath => app.config.loader_path = path,
                FileDialogTarget::ReplayPath => {
                    log::warn!("TODO: replay loading")
                }
            }
        };
    }
}

fn update_dl(dl_id: DownloadId) -> impl FnMut(Option<DownloadProgress>) {
    move |prog| {
        let dl_id = dl_id.clone();
        MESSAGES.push(Box::new(move |app| {
            let Frontend::GUI(app) = app else {
                return;
            };
            if let Some(progress) = prog {
                app.download_progress.insert(dl_id, progress);
            } else {
                app.download_progress.remove(&dl_id);
            }
        }))
    }
}

pub(crate) fn fetch_servers(hub: String) {
    wrap_async_task! {
        payload = anyhow::Result<IndexMap<String, ServerDisplay>>;
        task = move || {
            let response = utils::request(Url::try_from(&*hub)?, Method::GET, None::<SerdeNever>).await?;
            let data = utils::get_bytes(response, update_dl(DownloadId::ServerList)).await?;
            let servers = serde_json::from_slice::<Vec<ServerEntry>>(&data)?;
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
            Ok::<_, anyhow::Error>(servers)
        };
        handle = move |app, res| {
            let Frontend::GUI(app) = app else { return };
            match res {
                Ok(servers) => {
                    let mut hub = IndexSet::new();
                    for (url, server) in servers {
                        hub.insert(url.clone());
                        app.server_displays.insert(url, server);
                    }
                    app.hub_servers = HubStatus::Ok(hub)
                },
                Err(error) => {
                    app.hub_servers = HubStatus::Error(error)
                },
            }
        };
    }
}

pub enum TokenRefreshStatus {
    Success { refresh: RefreshResponse },
    Expired,
}

pub(crate) fn refresh_token(id: String, token: String, url: Url) {
    wrap_async_task! {
        payload = anyhow::Result<TokenRefreshStatus>;
        task = move || {
            let response = utils::request(url, Method::POST, Some(RefreshRequest { token })).await;
            match response {
                Ok(res) => {
                    let data = utils::get_bytes(res, |_| {}).await?;
                    let refresh = serde_json::from_slice::<RefreshResponse>(&data)?;
                    Ok(TokenRefreshStatus::Success { refresh })
                },
                Err(err) if let DeboaError::Response(ResponseError::Receive {
                    status_code: StatusCode::UNAUTHORIZED, ..
                }) = err => Ok(TokenRefreshStatus::Expired),
                Err(err) => return Err(err)?
            }
        };
        handle = move |app, res| {
            let Frontend::GUI(app) = app else { return };
            match res {
                Ok(TokenRefreshStatus::Success { refresh }) => {
                    let Some(account) = app.config.accounts.get_mut(&id) else {
                        return;
                    };
                    account.expire_time = refresh.expire_time;
                    account.token = refresh.new_token;
                },
                Ok(TokenRefreshStatus::Expired) => {
                    log::info!("account with ID {id} has an expired token");
                },
                Err(error) => {
                    log::warn!("failed to refresh token for account with ID {id}: {error}");
                }
            }
        };
    }
}

pub(crate) fn sync_signing_key(url: String) {
    wrap_async_task! {
        payload = anyhow::Result<String>;
        task = move || {
            let response = utils::request(Url::try_from(&*utils::sanitize_url(&url))?, Method::GET, None::<()>).await?;
            let data = utils::get_bytes(response, |_| {}).await?;
            let key = String::from_utf8_lossy(&*data).to_string();
            Ok(key)
        };
        handle = move |app, res| {
            match app {
                Frontend::GUI(app) => match res {
                    Ok(key) => app.config.signing_key = key,
                    Err(err) => app.sync_error = Some(err.to_string())
                },
                Frontend::CLI => todo!(),
            }
        };
    }
}

pub(crate) fn fetch_server_data(url: String) {
    let sane_url = Url::try_from(&*(utils::sanitize_url(&url) + "/status"));
    wrap_async_task! {
        payload = anyhow::Result<ServerStatus>;
        task = move || {
            match utils::request(
                sane_url?,
                Method::GET, None::<()>
            ).await {
                Ok(response) => {
                    let data = utils::get_bytes(response, |_| {}).await?;
                    let status = serde_json::from_slice::<ServerStatus>(&data)?;
                    Ok(status)
                },
                Err(err) => return Err(err)?
            }
        };
        handle = move |app, res| {
            let Frontend::GUI(app) = app else { return; };
            let Some(server) = app.get_server_display_mut(&url) else {
                log::warn!("fetched bad url: {url}");
                return;
            };
            server.status = match res {
                Ok(status) => FetchStatus::Fetched { status, info: InfoStatus::Unfetched },
                Err(err) => FetchStatus::FetchError(err.to_string())
            };
        };
    }
}

pub(crate) fn fetch_server_info(url: String) {
    let sane_url = Url::try_from(&*(utils::sanitize_url(&url) + "/info"));
    wrap_async_task! {
        payload = anyhow::Result<ServerInfo>;
        task = move || {
            match utils::request(
                sane_url?,
                Method::GET, None::<()>
            ).await {
                Ok(response) => {
                    let data = utils::get_bytes(response, |_| {}).await?;
                    let status = serde_json::from_slice::<ServerInfo>(&data)?;
                    Ok(status)
                },
                Err(err) => return Err(err)?
            }
        };
        handle = move |app, res| {
            let Frontend::GUI(app) = app else { return; };
            let Some(server) = app.get_server_display_mut(&url) else {
                log::warn!("fetched bad url: {url}");
                return;
            };
            let FetchStatus::Fetched { info: server_info, .. } = &mut server.status
            else { return; };
            *server_info = match res {
                Ok(info) => InfoStatus::Fetched(info),
                Err(err) => InfoStatus::FetchError(err.to_string())
            };
        };
    }
}

pub fn acquire(build: BuildInformation) {
    let db = DATABASE.wait();
    wrap_async_task! {
        payload = anyhow::Result<Vec<(Manifest, PathBuf)>>;
        task = move || {
            if let BuildInformation { manifest_url: Some(man), manifest_download_url: Some(man_dl), .. } = build {
                let sane_url = Url::parse(&utils::sanitize_url(&man))?;
                let response = utils::request(sane_url, Method::GET, None::<()>).await?;
                let manifest = utils::get_bytes(response, |_| {}).await?;
                if build.manifest_hash.is_some_and(|hash| hash != Manifest::digest(&*manifest)) {
                    log::error!("build manifest hash did not match expected");
                    anyhow::bail!("build manifest hash did not match expected");
                }
                let manifest = parse_manifest(manifest)?;
                let missing = manifest.iter().enumerate()
                    .filter_map(|(index, entry)| (!db.blob_exists(entry.0)).then_some(index))
                    .collect::<Vec<_>>();
                log::debug!("manifest contains {} files, we have {} of them", manifest.len(), missing.len());
                if missing.len() > 0 {
                    let man_dl = Url::parse(&utils::sanitize_url(&man_dl))?;
                    // why is this bytes and not just a json list. what
                    let mut body = Vec::new();
                    for index in &missing {
                        body.extend((*index as u32).to_le_bytes());
                    }
                    let req = deboa::request::post(man_dl)?
                        .header(HeaderName::from_static("Content-Type"), "application/octet-stream")?
                        .header(HeaderName::from_static("X-Robust-Download-Protocol"), "1")?
                        .body(HttpBody::bytes(&body));
                    let res = utils::request_raw(req).await?;
                    update_dl(DownloadId::BuildManifestCount)(Some(DownloadProgress { current: 0, total: Some(missing.len()) }));
                    let bytes = utils::get_bytes(res, update_dl(DownloadId::BuildManifest)).await?;
                    let mut bytes = Cursor::new(&*bytes);
                    let mut int_buf = [0u8; 4];
                    bytes.read_exact(&mut int_buf)?;
                    let flags = u32::from_le_bytes(int_buf);
                    for (i, index) in missing.iter().enumerate() {
                        bytes.read_exact(&mut int_buf)?;
                        let size = u32::from_le_bytes(int_buf) as usize;
                        let mut compressed = 0;
                        if flags & 1 != 0 {
                            bytes.read_exact(&mut int_buf)?;
                            compressed = u32::from_le_bytes(int_buf) as usize;
                        }
                        let mut raw = vec![0; if compressed == 0 { size } else { compressed }];
                        bytes.read_exact(&mut raw)?;
                        if flags & 1 != 0 && compressed != 0 {
                            let mut decoder = StreamingDecoder::new(Cursor::new(raw))?;
                            raw = vec![0; size];
                            decoder.read_exact(&mut raw)?;
                        }
                        db.put_blob(manifest[*index].0, raw)?;
                        update_dl(DownloadId::BuildManifestCount)(Some(DownloadProgress { current: i, total: Some(missing.len()) }));
                    }
                    update_dl(DownloadId::BuildManifestCount)(Some(DownloadProgress { current: missing.len(), total: Some(missing.len()) }));
                }
                return Ok(manifest)
            }
            let BuildInformation { download_url, hash, .. } = build;
            let sane_url = Url::parse(&utils::sanitize_url(&download_url))?;
            let response = utils::request(sane_url, Method::GET, None::<()>).await?;
            let zip_raw = utils::get_bytes(response, update_dl(DownloadId::BuildZip)).await?;
            let sha256 = sha256::digest(&*zip_raw);
            if sha256.to_ascii_lowercase() != hash.to_ascii_lowercase() {
                anyhow::bail!("sha256 mismatch (expected {hash}, got {sha256})");
            }
            let zip_cur = Cursor::new(zip_raw);
            let mut zip = ZipArchive::new(zip_cur)?;
            let files = zip.file_names().filter_map(|p| match p {
                Ok(v) => (!v.ends_with("/")).then(|| Ok(v.to_string())),
                Err(err) => Some(Err(err))
            }).collect::<Result<Vec<_>, _>>()?;
            let mut manifest = Vec::new();
            for path in files {
                let mut file = zip.by_name(&path)?;
                let mut bytes = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut bytes)?;
                let digest = Manifest::digest(&bytes);
                db.put_blob(digest, bytes)?;
                manifest.push((digest, PathBuf::from(path)));
            }
            Ok(manifest)
        };
        handle = move |app, _res| {
            match app {
                Frontend::GUI(app) => {},
                Frontend::CLI => {},
            }
        };
    }
}

fn parse_manifest(manifest: bytes::Bytes) -> Result<Vec<(Manifest, PathBuf)>, anyhow::Error> {
    let data = String::from_utf8(manifest.to_vec())?;
    let mut lines = data.lines();
    let Some("Robust Content Manifest 1") = lines.next() else {
        anyhow::bail!("incorrect header string")
    };
    lines.map(|line| {
        let Some((digest, path)) = line.split_once(' ') else { anyhow::bail!("invalid digest/path pair") };
        let hash = digest.parse()?;
        let path = path.parse()?;
        Ok((hash, path))
    }).collect()
}

pub fn ensure_engine(version: &str) {
    let version = version.to_string();
}
