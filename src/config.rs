use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{self, Read, Write},
    path::PathBuf,
};

use indexmap::IndexSet;

use crate::{models::Account, staticdata};

pub static DEFAULT_KEY_URL: &'static str = "https://raw.githubusercontent.com/space-wizards/SS14.Launcher/master/SS14.Launcher/signing_key";
fn key_url() -> String {
    DEFAULT_KEY_URL.into()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    #[serde(default)]
    pub hub_url: String,
    #[serde(default)]
    pub engine_manifest_url: String,
    #[serde(default)]
    pub auth_url: String,
    #[serde(default)]
    pub loader_path: PathBuf,
    #[serde(default)]
    pub accounts: HashMap<String, Account>,
    #[serde(default)]
    pub selected_account: Option<String>,
    #[serde(default)]
    pub pinned_servers: IndexSet<String>,
    #[serde(default)]
    pub signing_key: String,
    #[serde(default = "key_url")]
    pub key_url: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            hub_url: "https://hub.spacestation14.com/api/servers/".into(),
            engine_manifest_url: "https://robust-builds.cdn.spacestation14.com/manifest.json"
                .into(),
            auth_url: "https://auth.spacestation14.com/api/auth/".into(),
            loader_path: cfg_select! {
                target_os = "windows" => env::current_exe()
                    .expect("failed to get current exe")
                    .join("../loader/Robust.Loader.exe")
                    .canonicalize()
                    .expect("failed to get loader path"),
                target_os = "linux" => env::current_exe()
                    .expect("failed to get current exe")
                    .join("../loader/Robust.Loader")
                    .canonicalize()
                    .expect("failed to get loader path"),
                _ => "".into(),
            },
            accounts: HashMap::new(),
            selected_account: None,
            pinned_servers: IndexSet::new(),
            signing_key: String::new(),
            key_url: key_url(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Config, anyhow::Error> {
        let datapath = &staticdata::STATIC_DATA.datapath;
        match File::open(datapath.join("config.json")) {
            Ok(mut handle) => {
                let mut buf = String::new();
                handle
                    .read_to_string(&mut buf)
                    .inspect_err(|e| log::error!("failed to read config: {e}"))?;
                Ok(serde_json::from_str::<Config>(&buf)
                    .inspect_err(|e| log::error!("failed to load config: {e}"))?)
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let default_config = Config::default();
                default_config.save()?;
                Ok(default_config)
            }
            Err(err) => {
                log::error!("failed to read config: {err}");
                Err(err)?
            }
        }
    }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        let datapath = &staticdata::STATIC_DATA.datapath;
        let mut handle = File::create(datapath.join("config.json"))
            .inspect_err(|e| log::warn!("failed to create config: {e}"))?;
        handle
            .write_all(
                &serde_json::to_string_pretty(self)
                    .inspect_err(|e| log::warn!("failed to save config: {e}"))?
                    .into_bytes(),
            )
            .inspect_err(|e| log::warn!("failed to write config: {e}"))?;
        Ok(())
    }
}
