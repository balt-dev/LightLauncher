use std::{
    path::{Path, PathBuf},
    sync::{Mutex, atomic::AtomicBool},
};

use rusqlite::Connection;

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

    pub fn blob_exists(&self, digest: &str) -> bool {
        self.blobs.join(digest).exists()
    }
}
