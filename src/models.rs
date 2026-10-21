//! Contains data models for data being transmitted across the application and server.

use std::borrow::Cow;
use std::collections::HashMap;

use chrono::FixedOffset;

/// Live status information advertised by a game server.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct ServerStatus {
    pub map: Option<String>,
    pub name: Option<String>,
    pub tags: Vec<String>,
    pub preset: Option<String>,
    pub players: u32,
    pub round_id: Option<u32>,
    pub run_level: i32,
    pub panic_bunker: bool,
    pub soft_max_players: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct ServerListing {
    pub name: Option<String>,
    pub address: String,
}

/// A normalized server-list entry returned by a hub.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct ServerEntry {
    #[serde(flatten)]
    pub listing: ServerListing,
    pub inferred_tags: Vec<String>,
    #[serde(rename = "statusData")]
    pub status_data: ServerStatus,
}

impl ServerEntry {
    /// Gets a sensible display name for the given server entry.
    pub fn display_name(&self) -> Cow<'_, str> {
        self.listing
            .name
            .as_ref()
            .map(|s| Cow::<'_, str>::Borrowed(&*s))
            .or(self
                .status_data
                .name
                .as_ref()
                .map(|s| Cow::<'_, str>::Borrowed(&*s)))
            .unwrap_or_else(|| Cow::Owned(self.listing.address.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct BuildInformation {
    pub download_url: String,
    pub manifest_url: String,
    pub manifest_download_url: String,
    pub zip_hash: String,
    pub manifest_hash: String,
    pub version: String,
    pub fork_id: String,
    pub engine_version: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct AuthInformation {
    pub public_key: Option<String>,
}

#[derive(Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct Account {
    pub username: String,
    #[serde(rename = "userId")]
    pub user_id: String,
    pub token: String,
    #[serde(rename = "expireTime")]
    #[serde(with = "crate::serde::time")]
    pub expire_time: chrono::DateTime<FixedOffset>,
    #[serde(skip)]
    pub(crate) refreshing: bool,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account")
            .field("username", &self.username)
            .field("user_id", &self.user_id)
            .field("expire_time", &self.expire_time)
            .field("token", &..)
            .finish()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct ServerInfo {
    pub build: Option<BuildInformation>,
    pub auth: AuthInformation,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct ContentBundleMetadata {
    pub engine_version: String,
    pub base_build: Option<BuildInformation>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct EnginePlatformBuild {
    pub url: String,
    pub sha256: Option<String>,
    pub signature: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct EngineManifestEntry {
    pub redirect: Option<String>,
    pub insecure: bool,
    pub platforms: HashMap<String, EnginePlatformBuild>,
}

#[derive(Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct AuthRequest {
    pub username: String,
    pub password: String,
    #[serde(rename = "tfaCode")]
    #[serde(default)]
    pub tfa_code: String,
}

#[derive(Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct RefreshRequest {
    pub token: String,
}
#[derive(Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct RefreshResponse {
    #[serde(rename = "expireTime")]
    #[serde(with = "crate::serde::time")]
    pub expire_time: chrono::DateTime<FixedOffset>,
    #[serde(rename = "newToken")]
    pub new_token: String,
}

impl std::fmt::Debug for RefreshResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefreshResponse")
            .field("expire_time", &self.expire_time)
            .field("new_token", &..)
            .finish()
    }
}

impl std::fmt::Debug for AuthRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthRequest")
            .field("username", &self.username)
            .field("tfa_code", &self.tfa_code)
            .field("password", &..)
            .finish()
    }
}
impl std::fmt::Debug for RefreshRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefreshRequest")
            .field("token", &..)
            .finish()
    }
}
