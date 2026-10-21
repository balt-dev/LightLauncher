//! Contains data models for data being transmitted across the application and server.

use std::borrow::Cow;
use std::collections::HashMap;

use chrono::FixedOffset;

/// Live status information advertised by a game server.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct ServerStatus {
    #[serde(default)]
    pub map: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub players: u32,
    #[serde(default)]
    pub round_id: Option<u32>,
    #[serde(default)]
    pub run_level: i32,
    #[serde(default)]
    #[serde(with = "crate::serde::time")]
    pub round_start_time: chrono::DateTime<FixedOffset>,
    #[serde(default)]
    pub panic_bunker: bool,
    #[serde(default)]
    pub soft_max_players: Option<i32>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct ServerListing {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub address: String,
}

/// A normalized server-list entry returned by a hub.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct ServerEntry {
    #[serde(flatten)]
    #[serde(default)]
    pub listing: ServerListing,
    #[serde(rename = "inferredTags")]
    #[serde(default)]
    pub inferred_tags: Vec<String>,
    #[serde(rename = "statusData")]
    #[serde(default)]
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

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct BuildInformation {
    #[serde(default)]
    pub download_url: String,
    #[serde(default)]
    pub manifest_url: String,
    #[serde(default)]
    pub manifest_download_url: String,
    #[serde(default)]
    pub zip_hash: String,
    #[serde(default)]
    pub manifest_hash: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub fork_id: String,
    #[serde(default)]
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
    #[serde(default)]
    pub build: Option<BuildInformation>,
    #[serde(default)]
    pub auth: AuthInformation,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct ContentBundleMetadata {
    #[serde(default)]
    pub engine_version: String,
    #[serde(default)]
    pub base_build: Option<BuildInformation>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct EnginePlatformBuild {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct EngineManifestEntry {
    #[serde(default)]
    pub redirect: Option<String>,
    #[serde(default)]
    pub insecure: bool,
    #[serde(default)]
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
    #[serde(default)]
    pub expire_time: chrono::DateTime<FixedOffset>,
    #[serde(rename = "newToken")]
    #[serde(default)]
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
