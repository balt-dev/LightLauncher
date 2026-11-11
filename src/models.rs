//! Contains data models for data being transmitted across the application and server.

use std::borrow::Cow;
use std::collections::HashMap;
use std::str::FromStr;

use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest as _};
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
    #[serde(with = "crate::serde::time_opt")]
    pub round_start_time: Option<chrono::DateTime<FixedOffset>>,
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

impl ServerListing {
    /// Gets a sensible display name for the given server entry.
    pub fn display_name<'b, 'a: 'b>(&'a self, fallback: Option<String>) -> Cow<'b, str> {
        self.name
            .as_ref()
            .map(|s| Cow::<'_, str>::Borrowed(&*s))
            .or(fallback.map(Cow::<'_, str>::Owned))
            .unwrap_or_else(|| Cow::Owned(self.address.to_string()))
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct BuildInformation {
    #[serde(default)]
    pub download_url: String,
    #[serde(default)]
    pub manifest_url: Option<String>,
    #[serde(default)]
    pub manifest_download_url: Option<String>,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub manifest_hash: Option<Digest>,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub fork_id: String,
    #[serde(default)]
    pub engine_version: String,
    #[serde(default)]
    pub acz: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct AuthInformation {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
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
    #[serde(default)]
    pub links: Vec<ServerLink>,
    #[serde(default)]
    pub privacy_policy: Option<PrivacyPolicy>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub connect_address: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct PrivacyPolicy {
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub link: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct ServerLink {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
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

#[derive(Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Hash)]
#[allow(missing_docs)]
pub struct AuthRequest {
    pub username: String,
    pub password: String,
    #[serde(rename = "tfaCode")]
    #[serde(default)]
    pub tfa_code: String,
}

#[derive(Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Hash)]
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

#[derive(Default, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InfoStatus {
    #[default]
    Unfetched,
    Fetching,
    Fetched(ServerInfo),
    FetchError(String),
}

#[derive(Default, Debug)]
pub enum FetchStatus {
    #[default]
    Unfetched,
    Fetching,
    Fetched {
        status: ServerStatus,
        info: InfoStatus,
    },
    FetchError(String),
}

#[derive(Default, Debug)]
pub struct ServerDisplay {
    pub listing: ServerListing,
    pub status: FetchStatus,
}

impl ServerDisplay {
    pub(crate) fn display_name(&self) -> &str {
        match &self.status {
            FetchStatus::Fetched { status, .. } if let Some(name) = &status.name => &name,
            _ if let Some(listname) = &self.listing.name => &listname,
            _ => &self.listing.address,
        }
    }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Digest([u8; 32]);

impl AsRef<[u8; 32]> for Digest {
    fn as_ref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Digest {
    pub fn digest(data: &[u8]) -> Self {
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(&data);
        let digest = hasher.finalize();
        Self(
            *digest
                .as_array()
                .expect("U32 type should guarantee this slice is 32 bytes long"),
        )
    }
}

impl PartialEq<[u8]> for Digest {
    fn eq(&self, other: &[u8]) -> bool {
        let Some(arr): Option<&[u8; 32]> = other.as_array() else {
            return false;
        };
        self.0.eq(arr)
    }
}

impl std::fmt::Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            &self
                .0
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<String>(),
        )
    }
}

impl serde::Serialize for Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(DigestVisitor)
    }
}

struct DigestVisitor;
impl<'de> serde::de::Visitor<'de> for DigestVisitor {
    type Value = Digest;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "a string representing a 32-byte Blake2b hash")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.parse::<Digest>().map_err(E::custom)
    }
}

impl FromStr for Digest {
    type Err = anyhow::Error;
    fn from_str(v: &str) -> Result<Self, Self::Err> {
        if v.len() != 64 || !v.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            anyhow::bail!("invalid manifest string")
        }
        let mut arr = [0; 32];
        for i in 0..32 {
            let byte = u8::from_str_radix(&v[i * 2..i * 2 + 2], 16)
                .expect("we already checked the whole string");
            arr[i] = byte;
        }
        Ok(Digest(arr))
    }
}
