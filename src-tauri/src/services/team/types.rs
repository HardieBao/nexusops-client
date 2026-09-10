use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProfileKey {
    pub id: i64,
    pub name: String,
    pub prefix: String,
    pub last_four: String,
    pub platform: String,
    pub allow_messages_dispatch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProfileModel {
    pub id: String,
    pub name: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssetLimits {
    pub max_assets: u64,
    pub max_revisions_per_asset: u64,
    pub max_text_bytes: u64,
    pub max_archive_bytes: u64,
    pub max_unpacked_bytes: u64,
    pub max_files: u64,
    pub max_path_depth: u64,
    pub max_path_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TeamProfile {
    pub schema_version: u32,
    pub organization_id: String,
    pub workspace_id: String,
    pub name: String,
    pub gateway_url: String,
    pub base_url: String,
    pub key: ProfileKey,
    pub models: Vec<ProfileModel>,
    pub asset_manifest_url: String,
    pub subscriptions: Vec<String>,
    pub limits: AssetLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Prompt,
    Skill,
    Rule,
    Workflow,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManifestItem {
    pub asset_id: i64,
    pub kind: AssetKind,
    pub slug: String,
    pub name: String,
    pub revision: i64,
    pub content_hash: String,
    pub archive_sha256: Option<String>,
    pub download_url: String,
    pub content_type: String,
    pub byte_size: u64,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManifestConflict {
    pub asset_id: i64,
    pub slug: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub assets: Vec<ManifestItem>,
    pub conflicts: Vec<ManifestConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Connected,
    AuthenticationRequired,
    AccessDenied,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamConnection {
    pub id: String,
    pub gateway_url: String,
    pub profile: TeamProfile,
    pub status: ConnectionStatus,
    pub last_checked_at: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedAssetState {
    pub connection_id: String,
    pub app: String,
    pub asset_id: i64,
    #[serde(default)]
    pub asset_name: String,
    #[serde(default)]
    pub asset_kind: Option<AssetKind>,
    pub revision: i64,
    pub content_hash: String,
    pub archive_sha256: Option<String>,
    #[serde(default)]
    pub asset_files: Vec<FileEntry>,
    pub local_hash: String,
    pub install_path: String,
    #[serde(default)]
    pub install_root: String,
    #[serde(default)]
    pub relative_path: String,
    #[serde(default)]
    pub backup_path: Option<String>,
    #[serde(default)]
    pub backup_root_path: Option<String>,
    #[serde(default)]
    pub backup_created_at: Option<String>,
    #[serde(default)]
    pub install_operation: Option<String>,
    #[serde(default)]
    pub upstream_pending: bool,
    #[serde(default)]
    pub upstream_operation: Option<String>,
    #[serde(default)]
    pub upstream_backup_path: Option<String>,
    #[serde(default)]
    pub upstream_fingerprint: Option<String>,
    #[serde(default)]
    pub shared_upstream_fingerprint: Option<String>,
    #[serde(default)]
    pub pending_previous_upstream_fingerprint: Option<String>,
    #[serde(default)]
    pub pending_target_upstream_fingerprint: Option<String>,
    #[serde(default)]
    pub restored_unmanaged: bool,
    #[serde(default)]
    pub pending_disk_state: Option<PendingDiskState>,
    #[serde(default)]
    pub pending_limits: Option<AssetLimits>,
    pub subscribed: bool,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "hash", rename_all = "snake_case")]
pub enum PendingDiskState {
    Absent,
    Hash(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedProviderLink {
    pub connection_id: String,
    pub app: String,
    pub provider_id: String,
    #[serde(default)]
    pub managed_model: Option<String>,
}

#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TeamError {
    #[error("Enter an HTTP(S) gateway service URL without credentials, query or fragment")]
    InvalidGateway,
    #[error("Remote gateways require HTTPS; HTTP is allowed only for literal loopback addresses")]
    InsecureGateway,
    #[error("The response URL is outside the connected gateway API")]
    ForeignUrl,
    #[error("Redirects are not accepted for authenticated Team requests")]
    Redirect,
    #[error("The member key is invalid, expired or revoked; reconnect with a current key")]
    AuthenticationRequired,
    #[error("The member key cannot access this resource")]
    AccessDenied,
    #[error("The authorized asset release is no longer available; refresh the manifest")]
    NotFound,
    #[error("The server state changed; refresh before retrying")]
    Conflict,
    #[error("This gateway does not support the required TeamAI protocol; upgrade the gateway before using this feature")]
    UpgradeRequired,
    #[error("The gateway rate limit was reached; retry after {retry_after_seconds} seconds")]
    RateLimited { retry_after_seconds: u64 },
    #[error("The gateway is temporarily unavailable")]
    Unavailable,
    #[error("The request timed out")]
    Timeout,
    #[error("The operation was cancelled")]
    Cancelled,
    #[error("The response exceeds the supported size limit")]
    TooLarge,
    #[error("The gateway returned an invalid or unsupported Team response")]
    InvalidResponse,
    #[error("The operating system credential store is unavailable")]
    CredentialStore,
    #[error("The local Team database is unavailable")]
    Storage,
    #[error("An interrupted Team install conflicts with a newer local edit; the current target and backup were preserved")]
    RecoveryConflict,
    #[error("Connect to a Team gateway first")]
    NotConnected,
    #[error(
        "Disconnect the current Team before connecting to a different gateway or organization"
    )]
    DifferentIdentity,
}

impl TeamError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidGateway => "invalid_gateway",
            Self::InsecureGateway => "insecure_gateway",
            Self::ForeignUrl => "foreign_url",
            Self::Redirect => "redirect",
            Self::AuthenticationRequired => "authentication_required",
            Self::AccessDenied => "access_denied",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::UpgradeRequired => "upgrade_required",
            Self::RateLimited { .. } => "rate_limited",
            Self::Unavailable => "unavailable",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::TooLarge => "too_large",
            Self::InvalidResponse => "invalid_response",
            Self::CredentialStore => "credential_store",
            Self::Storage => "storage",
            Self::RecoveryConflict => "recovery_conflict",
            Self::NotConnected => "not_connected",
            Self::DifferentIdentity => "different_identity",
        }
    }
}

#[cfg(test)]
pub(crate) fn fixture_profile() -> TeamProfile {
    TeamProfile {
        schema_version: 1,
        organization_id: "local".into(),
        workspace_id: "local".into(),
        name: "Synthetic Team".into(),
        gateway_url: "https://gateway.example.test/".into(),
        base_url: "https://provider.example.test/v1".into(),
        key: ProfileKey {
            id: 7,
            name: "Fixture key".into(),
            prefix: "nx_".into(),
            last_four: "test".into(),
            platform: "openai".into(),
            allow_messages_dispatch: false,
        },
        models: vec![],
        asset_manifest_url: "/api/v1/me/assets/manifest".into(),
        subscriptions: vec![],
        limits: AssetLimits {
            max_assets: 1000,
            max_revisions_per_asset: 1000,
            max_text_bytes: 1 << 20,
            max_archive_bytes: 20 << 20,
            max_unpacked_bytes: 100 << 20,
            max_files: 2000,
            max_path_depth: 16,
            max_path_bytes: 240,
        },
    }
}
