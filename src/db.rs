use std::{
    collections::HashMap, fs::File, io::{self, Cursor, Read, Write}, path::{Path, PathBuf}, sync::{Mutex, atomic::AtomicBool},
};

use blake2::{Blake2b, Digest, digest::consts::U32};
use indexmap::IndexMap;
use reqwest::{Method, Url};
use rusqlite::Connection;
use ruzstd::decoding::StreamingDecoder;
use serde::Deserialize;

use crate::{models::{BuildInformation, EngineManifestEntry, Manifest}, utils::{self, HTTP_CLIENT, SerdeNever}};

static DB_INIT: AtomicBool = AtomicBool::new(false);

pub struct Database {
    root: PathBuf,
    blobs: PathBuf,
    conn: Mutex<Connection>,
}

impl Database {
    pub fn init(datapath: &Path) -> Result<Database, anyhow::Error> {
        if DB_INIT.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(anyhow::anyhow!("database already initialized"));
        }
        DB_INIT.store(true, std::sync::atomic::Ordering::Relaxed);

        let root = datapath.to_path_buf();
        let blobs = datapath.join("blobs");

        std::fs::create_dir_all(&blobs)?;

        let conn = Connection::open(datapath.join("content.db"))?;

        conn.execute_batch(
            "
            BEGIN;
            PRAGMA foreign_keys = ON;
            
            CREATE TABLE IF NOT EXISTS Files(Path TEXT PRIMARY KEY, Digest TEXT NOT NULL);
    
            CREATE TABLE IF NOT EXISTS ContentVersion(
                Id INTEGER PRIMARY KEY AUTOINCREMENT,
                Hash BLOB NOT NULL,
                ForkId TEXT,
                ForkVersion TEXT,
                LastUsed TEXT NOT NULL,
                ZipHash BLOB
            );
    
            CREATE TABLE IF NOT EXISTS Content(
                Id INTEGER PRIMARY KEY AUTOINCREMENT,
                Hash BLOB NOT NULL UNIQUE,
                Size INTEGER NOT NULL,
                Compression INTEGER NOT NULL,
                Data BLOB NOT NULL
            );
    
            CREATE TABLE IF NOT EXISTS ContentManifest(
                Id INTEGER PRIMARY KEY AUTOINCREMENT,
                VersionId INTEGER NOT NULL REFERENCES ContentVersion(Id)
                    ON DELETE CASCADE,
                Path TEXT NOT NULL,
                ContentId INTEGER NOT NULL REFERENCES Content(Id)
                    ON DELETE RESTRICT,
                UNIQUE(VersionId, Path)
            );
    
            CREATE TABLE IF NOT EXISTS ContentEngineDependency(
                Id INTEGER PRIMARY KEY AUTOINCREMENT,
                VersionId INTEGER NOT NULL REFERENCES ContentVersion(Id)
                    ON DELETE CASCADE,
                ModuleName TEXT NOT NULL,
                ModuleVersion TEXT NOT NULL,
                UNIQUE(VersionId, ModuleName)
            );
    
            CREATE TABLE IF NOT EXISTS InterruptedDownload(
                Id INTEGER PRIMARY KEY AUTOINCREMENT,
                Added TEXT NOT NULL
            );
    
            CREATE TABLE IF NOT EXISTS InterruptedDownloadContent(
                Id INTEGER PRIMARY KEY AUTOINCREMENT,
                InterruptedDownloadId INTEGER NOT NULL
                    REFERENCES InterruptedDownload(Id) ON DELETE CASCADE,
                ContentId INTEGER NOT NULL UNIQUE
                    REFERENCES Content(Id) ON DELETE CASCADE
            );
    
            CREATE TABLE IF NOT EXISTS RunningClient(
                ProcessId INTEGER PRIMARY KEY NOT NULL,
                MainModule TEXT NOT NULL,
                UsedVersion INTEGER NOT NULL
                    REFERENCES ContentVersion(Id) ON DELETE RESTRICT
            );
            COMMIT;
        ",
        )?;

        Ok(Database {
            root,
            blobs,
            conn: Mutex::new(conn),
        })
    }

    pub fn blob_exists(&self, digest: Manifest) -> bool {
        self.blobs.join(&digest.to_string()).exists()
    }
    
    pub fn put_blob(&self, check: Manifest, data: Vec<u8>) -> Result<(), io::Error> {
        log::debug!("got content blob {check} ({} bytes)", data.len());
        let hexdigest = Manifest::digest(&data);
        if check != hexdigest {
            log::warn!("content blob hash mismatch (expected {check:?}, got {hexdigest:?})");
            return Err(io::Error::new(io::ErrorKind::InvalidData, "content blob hash mismatch"));
        }
        if !self.blob_exists(hexdigest) {
            let mut fs = File::create(self.blobs.join(hexdigest.to_string()))?;
            fs.write_all(&data)?;
            fs.flush()?;
        }
        Ok(())
    }
    
    pub async fn acquire(&self, build: BuildInformation) -> Result<Vec<(Manifest, PathBuf)>, anyhow::Error> {
        if let BuildInformation { manifest_url: Some(man), manifest_download_url: Some(man_dl), .. } = build {
            let manifest = utils::request_bytes::<SerdeNever>(Url::parse(&utils::sanitize_url(&man))?, None, Method::GET).await?;
            if build.manifest_hash.is_some_and(|hash| hash != Manifest::digest(&*manifest)) {
                log::error!("build manifest hash did not match expected");
                anyhow::bail!("build manifest hash did not match expected");
            }
            let manifest = self.parse_manifest(manifest)?;
            let missing = manifest.iter().enumerate()
                .filter_map(|(index, entry)| (!self.blob_exists(entry.0)).then_some(index))
                .collect::<Vec<_>>();
            log::debug!("manifest contains {} files, we have {} of them", manifest.len(), missing.len());
            if missing.len() > 0 {
                let man_dl = Url::parse(&utils::sanitize_url(&man_dl))?;
                // why is this bytes and not just a json list. what
                let mut body = Vec::new();
                for index in &missing {
                    body.extend((*index as u32).to_le_bytes());
                }
                let res = HTTP_CLIENT
                    .request(Method::POST, man_dl)
                    .body(body)
                    .header("Content-Type", "application/octet-stream")
                    .header("X-Robust-Download-Protocol", "1")
                    .send().await.and_then(|r| r.error_for_status())?;
                let bytes = res.bytes().await?;
                let mut bytes = Cursor::new(&*bytes);
                let mut int_buf = [0u8; 4];
                bytes.read_exact(&mut int_buf)?;
                let flags = u32::from_le_bytes(int_buf);
                for index in &missing {
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
                    self.put_blob(manifest[*index].0, raw)?;
                }
            }
            return Ok(manifest)
        }
        let BuildInformation { download_url, hash, .. } = build;
        todo!()
    }

    fn parse_manifest(&self, manifest: bytes::Bytes) -> Result<Vec<(Manifest, PathBuf)>, anyhow::Error> {
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
}
