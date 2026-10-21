use std::{path::PathBuf, sync::LazyLock, time::Duration};

use chrono::TimeDelta;
use egui::{Id, LayerId, Order};

use crate::db::Database;

pub const TOKEN_REFRESH_INTERVAL: Duration = Duration::from_mins(5);
pub const TOKEN_REFRESH_THRESHOLD: chrono::TimeDelta = TimeDelta::days(5);
pub const AUTOSAVE_INTERVAL: std::time::Duration = Duration::from_secs(30);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub static USER_AGENT: &'static str = concat!("LightLauncher/", env!("CARGO_PKG_VERSION"));

pub struct StaticData {
    pub datapath: PathBuf,
    pub subwindow_layer: LayerId,
    pub db: Database,
}

pub static STATIC_DATA: LazyLock<StaticData> = LazyLock::new(|| {
    let Some(dirs) = directories::ProjectDirs::from("sno.mba", "baltdev", "LightLauncher") else {
        eprintln!("failed during init: could not find user directories");
        std::process::exit(1);
    };
    let datapath = dirs.data_local_dir();
    match std::fs::create_dir_all(datapath) {
        Ok(()) => {}
        Err(err) => {
            eprintln!("failed during init: could not create directory: {err}");
            std::process::exit(1);
        }
    }
    let db = match Database::init(&datapath.join("data")) {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("failed during init: could not open database: {err}");
            std::process::exit(1);
        }
    };
    StaticData {
        datapath: datapath.to_path_buf(),
        subwindow_layer: LayerId {
            order: Order::Foreground,
            id: Id::unique("subwindow"),
        },
        db,
    }
});
