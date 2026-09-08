pub mod api;
pub mod content;
pub mod credentials;
pub mod install;
pub mod provider;
pub mod state;
pub mod sync;
pub mod types;

use std::{path::Path, sync::Arc};

use chrono::Utc;
use sha2::{Digest, Sha256};

use api::{Cancellation, TeamApi};
use credentials::{CredentialStore, OsCredentialStore};
use state::TeamState;
use types::{ConnectionStatus, Manifest, ManifestItem, TeamConnection, TeamError, TeamProfile};

pub struct TeamService {
    pub state: TeamState,
    credentials: Arc<dyn CredentialStore>,
    operation: tokio::sync::Mutex<()>,
    pub(crate) sync_operation: tokio::sync::Mutex<()>,
}

impl TeamService {
    pub fn open(data_dir: &Path) -> Result<Self, TeamError> {
        Self::with_credentials(data_dir, Arc::new(OsCredentialStore))
    }

    pub fn with_credentials(
        data_dir: &Path,
        credentials: Arc<dyn CredentialStore>,
    ) -> Result<Self, TeamError> {
        let state = TeamState::open(data_dir)?;
        sync::recover_known_installs(&state)?;
        Ok(Self {
            state,
            credentials,
            operation: tokio::sync::Mutex::new(()),
            sync_operation: tokio::sync::Mutex::new(()),
        })
    }

    pub fn status(&self) -> Result<Option<TeamConnection>, TeamError> {
        self.load_connection()
    }

    fn load_connection(&self) -> Result<Option<TeamConnection>, TeamError> {
        let connection = self.state.connection()?;
        if let Some(current) = &connection {
            let gateway = api::normalize_gateway(&current.gateway_url)?;
            if gateway.as_str() != current.gateway_url
                || connection_id(gateway.as_str(), &current.profile)? != current.id
            {
                return Err(TeamError::DifferentIdentity);
            }
        }
        Ok(connection)
    }

    pub async fn connect(
        &self,
        gateway: &str,
        key: &str,
        cancel: &Cancellation,
    ) -> Result<TeamConnection, TeamError> {
        let _sync = tokio::select! {
            _ = cancel.cancelled() => return Err(TeamError::Cancelled),
            lock = self.sync_operation.lock() => lock,
        };
        let _operation = tokio::select! {
            _ = cancel.cancelled() => return Err(TeamError::Cancelled),
            lock = self.operation.lock() => lock,
        };
        let api = TeamApi::new(gateway)?;
        let previous = self.load_connection()?;
        if previous
            .as_ref()
            .is_some_and(|current| current.gateway_url != api.gateway_url())
        {
            return Err(TeamError::DifferentIdentity);
        }
        let profile = api.profile(key, cancel).await?;
        if previous
            .as_ref()
            .is_some_and(|current| !same_scope(&current.profile, &profile))
        {
            return Err(TeamError::DifferentIdentity);
        }
        if cancel.is_cancelled() {
            return Err(TeamError::Cancelled);
        }
        let id = connection_id(api.gateway_url(), &profile)?;
        let previous_key = self.credentials.get(&id)?;
        self.credentials.set(&id, key)?;
        let connection = TeamConnection {
            id: id.clone(),
            gateway_url: api.gateway_url().into(),
            profile,
            status: ConnectionStatus::Connected,
            last_checked_at: Utc::now().to_rfc3339(),
            last_error: None,
        };
        if let Err(error) = self.state.save_connection(&connection) {
            // No plaintext credential is put in SQLite, even during compensation.
            if let Some(previous_key) = previous_key {
                self.credentials.set(&id, &previous_key)?;
            } else {
                self.credentials.delete(&id)?;
            }
            return Err(error);
        }
        Ok(connection)
    }

    pub async fn refresh(
        &self,
        cancel: &Cancellation,
    ) -> Result<(TeamConnection, Manifest), TeamError> {
        let _operation = tokio::select! {
            _ = cancel.cancelled() => return Err(TeamError::Cancelled),
            lock = self.operation.lock() => lock,
        };
        let mut connection = self.load_connection()?.ok_or(TeamError::NotConnected)?;
        let result = async {
            let key = self
                .credentials
                .get(&connection.id)?
                .ok_or(TeamError::AuthenticationRequired)?;
            let api = TeamApi::new(&connection.gateway_url)?;
            let profile = api.profile(&key, cancel).await?;
            if !same_scope(&connection.profile, &profile)
                || profile.key.id != connection.profile.key.id
            {
                return Err(TeamError::DifferentIdentity);
            }
            let manifest = api.manifest(&key, cancel).await?;
            validate_manifest_limits(&profile, &manifest)?;
            Ok((profile, manifest))
        }
        .await;
        match result {
            Ok((profile, manifest)) => {
                connection.profile = profile;
                connection.status = ConnectionStatus::Connected;
                connection.last_checked_at = Utc::now().to_rfc3339();
                connection.last_error = None;
                self.state.save_connection(&connection)?;
                Ok((connection, manifest))
            }
            Err(error) => {
                if error != TeamError::Cancelled {
                    connection.status = match error {
                        TeamError::AuthenticationRequired | TeamError::DifferentIdentity => {
                            ConnectionStatus::AuthenticationRequired
                        }
                        TeamError::AccessDenied => ConnectionStatus::AccessDenied,
                        _ => ConnectionStatus::Unavailable,
                    };
                    connection.last_error = Some(error.code().into());
                    self.state.save_connection(&connection)?;
                }
                Err(error)
            }
        }
    }

    pub async fn download(
        &self,
        item: &ManifestItem,
        cancel: &Cancellation,
    ) -> Result<Vec<u8>, TeamError> {
        let _operation = tokio::select! {
            _ = cancel.cancelled() => return Err(TeamError::Cancelled),
            lock = self.operation.lock() => lock,
        };
        let mut connection = self.load_connection()?.ok_or(TeamError::NotConnected)?;
        let key = self
            .credentials
            .get(&connection.id)?
            .ok_or(TeamError::AuthenticationRequired)?;
        let limit = if item.kind == types::AssetKind::Skill {
            connection.profile.limits.max_archive_bytes
        } else {
            connection.profile.limits.max_text_bytes
        };
        let result = TeamApi::new(&connection.gateway_url)?
            .download(item, &key, limit, cancel)
            .await;
        if let Err(error) = &result {
            if matches!(
                error,
                TeamError::AuthenticationRequired | TeamError::AccessDenied
            ) {
                connection.status = if *error == TeamError::AuthenticationRequired {
                    ConnectionStatus::AuthenticationRequired
                } else {
                    ConnectionStatus::AccessDenied
                };
                connection.last_error = Some(error.code().into());
                self.state.save_connection(&connection)?;
            }
        }
        result
    }

    // Rust-only provider import uses the existing upstream provider formats.
    // Never expose this method as a Tauri command returning a secret to the UI.
    pub fn member_key(&self) -> Result<String, TeamError> {
        let connection = self.load_connection()?.ok_or(TeamError::NotConnected)?;
        self.credentials
            .get(&connection.id)?
            .ok_or(TeamError::AuthenticationRequired)
    }

    #[cfg(test)]
    pub async fn disconnect(&self) -> Result<(), TeamError> {
        let _sync = self.sync_operation.lock().await;
        self.disconnect_locked().await
    }

    pub(crate) async fn disconnect_locked(&self) -> Result<(), TeamError> {
        let _operation = self.operation.lock().await;
        if let Some(connection) = self.state.connection()? {
            self.credentials.delete(&connection.id)?;
            self.state.disconnect(&connection.id)?;
        }
        Ok(())
    }
}

fn same_scope(left: &TeamProfile, right: &TeamProfile) -> bool {
    left.organization_id == right.organization_id && left.workspace_id == right.workspace_id
}

fn validate_manifest_limits(profile: &TeamProfile, manifest: &Manifest) -> Result<(), TeamError> {
    if manifest.assets.len() as u64 > profile.limits.max_assets {
        return Err(TeamError::InvalidResponse);
    }
    let limits = content::ContentLimits::try_from(&profile.limits)
        .map_err(|_| TeamError::InvalidResponse)?;
    for item in &manifest.assets {
        content::validate_manifest_item(item, limits).map_err(|_| TeamError::InvalidResponse)?;
        let max_bytes = if item.kind == types::AssetKind::Skill {
            profile.limits.max_archive_bytes
        } else {
            profile.limits.max_text_bytes
        };
        if item.byte_size > max_bytes || item.files.len() as u64 > profile.limits.max_files {
            return Err(TeamError::InvalidResponse);
        }
    }
    Ok(())
}

fn connection_id(gateway: &str, profile: &TeamProfile) -> Result<String, TeamError> {
    let identity = serde_json::to_vec(&(gateway, &profile.organization_id, &profile.workspace_id))
        .map_err(|_| TeamError::InvalidResponse)?;
    Ok(format!("{:x}", Sha256::digest(identity)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::HeaderMap, response::IntoResponse, routing::get, Json, Router};
    use credentials::fixtures::MemoryCredentialStore;
    use std::sync::Mutex;
    use types::{fixture_profile, ManagedAssetState, ManagedProviderLink};

    #[tokio::test]
    async fn connection_rotation_restart_and_disconnect_keep_secrets_out_of_sqlite() {
        let profile = Arc::new(Mutex::new(fixture_profile()));
        let expected_key = Arc::new(Mutex::new("nx_synthetic_member_one".to_owned()));
        let source_profile = profile.clone();
        let source_key = expected_key.clone();
        let router = Router::new()
            .route("/edge/api/v1/me/team-profile", get(move |headers: HeaderMap| async move {
                if headers.get("X-Nexus-Member-Key").and_then(|value| value.to_str().ok()) != Some(source_key.lock().unwrap().as_str()) {
                    return axum::http::StatusCode::UNAUTHORIZED.into_response();
                }
                Json(serde_json::json!({"code":0,"data":source_profile.lock().unwrap().clone()})).into_response()
            }))
            .route("/edge/api/v1/me/assets/manifest", get(|| async {
                Json(serde_json::json!({"code":0,"data":{"schema_version":1,"assets":[],"conflicts":[]}}))
            }));
        let (origin, server) = api::tests::serve(router).await;
        let gateway = format!("{origin}edge/");
        profile.lock().unwrap().gateway_url = gateway.clone();
        let directory = tempfile::tempdir().unwrap();
        let credentials = Arc::new(MemoryCredentialStore::default());
        let team = TeamService::with_credentials(directory.path(), credentials.clone()).unwrap();
        let cancel = Cancellation::default();
        let first = team
            .connect(&gateway, "nx_synthetic_member_one", &cancel)
            .await
            .unwrap();
        assert!(first.profile.models.is_empty());
        assert_eq!(first.profile.key.platform, "openai");
        assert_eq!(first.gateway_url, gateway);
        let mut tampered = first.clone();
        tampered.gateway_url = "https://another.example/".into();
        team.state.save_connection(&tampered).unwrap();
        assert_eq!(
            team.refresh(&cancel).await.unwrap_err(),
            TeamError::DifferentIdentity
        );
        team.state.save_connection(&first).unwrap();
        let file = directory.path().join("managed-command.md");
        std::fs::write(&file, "developer file survives disconnect").unwrap();
        team.state
            .save_asset_state(&ManagedAssetState {
                connection_id: first.id.clone(),
                app: "claude".into(),
                asset_id: 1,
                asset_name: "Managed command".into(),
                asset_kind: Some(types::AssetKind::Rule),
                revision: 2,
                content_hash: "a".repeat(64),
                archive_sha256: None,
                asset_files: Vec::new(),
                local_hash: "b".repeat(64),
                install_path: file.to_string_lossy().into(),
                install_root: directory.path().to_string_lossy().into(),
                relative_path: "managed-command.md".into(),
                backup_path: None,
                backup_root_path: None,
                backup_created_at: None,
                install_operation: None,
                upstream_pending: false,
                upstream_operation: None,
                upstream_backup_path: None,
                upstream_fingerprint: None,
                shared_upstream_fingerprint: None,
                pending_previous_upstream_fingerprint: None,
                pending_target_upstream_fingerprint: None,
                restored_unmanaged: false,
                pending_disk_state: None,
                pending_limits: None,
                subscribed: true,
                last_synced_at: None,
            })
            .unwrap();
        team.state
            .save_provider_link(&ManagedProviderLink {
                connection_id: first.id.clone(),
                app: "claude".into(),
                provider_id: "owned-provider".into(),
                managed_model: None,
            })
            .unwrap();
        *expected_key.lock().unwrap() = "nx_synthetic_member_two".into();
        profile.lock().unwrap().key.id = 8;
        assert_eq!(
            team.refresh(&cancel).await.unwrap_err(),
            TeamError::AuthenticationRequired
        );
        assert_eq!(
            team.status().unwrap().unwrap().status,
            ConnectionStatus::AuthenticationRequired
        );
        let rotated = team
            .connect(&gateway, "nx_synthetic_member_two", &cancel)
            .await
            .unwrap();
        assert_eq!(first.id, rotated.id);
        assert_eq!(credentials.0.lock().unwrap().len(), 1);
        drop(team);
        let resumed = TeamService::with_credentials(directory.path(), credentials.clone()).unwrap();
        assert_eq!(resumed.refresh(&cancel).await.unwrap().0.profile.key.id, 8);
        for entry in std::fs::read_dir(directory.path()).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                let bytes = std::fs::read(path).unwrap();
                for secret in [
                    b"nx_synthetic_member_one".as_slice(),
                    b"nx_synthetic_member_two".as_slice(),
                ] {
                    assert!(!bytes.windows(secret.len()).any(|window| window == secret));
                }
            }
        }
        resumed.disconnect().await.unwrap();
        assert!(resumed.status().unwrap().is_none());
        assert!(credentials.0.lock().unwrap().is_empty());
        assert!(!resumed.state.asset_states(&first.id).unwrap()[0].subscribed);
        assert_eq!(
            resumed.state.provider_links(&first.id).unwrap()[0].provider_id,
            "owned-provider"
        );
        assert_eq!(
            std::fs::read_to_string(file).unwrap(),
            "developer file survives disconnect"
        );
        server.abort();
    }

    #[tokio::test]
    async fn a_different_server_identity_requires_explicit_disconnect() {
        let profile = Arc::new(Mutex::new(fixture_profile()));
        let response = profile.clone();
        let (gateway, server) = api::tests::serve(Router::new().route(
            "/api/v1/me/team-profile",
            get(move || async move {
                Json(serde_json::json!({"code":0,"data":response.lock().unwrap().clone()}))
            }),
        ))
        .await;
        profile.lock().unwrap().gateway_url = gateway.clone();
        let directory = tempfile::tempdir().unwrap();
        let credentials = Arc::new(MemoryCredentialStore::default());
        let team = TeamService::with_credentials(directory.path(), credentials.clone()).unwrap();
        let cancel = Cancellation::default();
        let original = team
            .connect(&gateway, "nx_synthetic_fixture", &cancel)
            .await
            .unwrap();
        profile.lock().unwrap().organization_id = "another-organization".into();
        assert_eq!(
            team.connect(&gateway, "nx_new_synthetic_fixture", &cancel)
                .await
                .unwrap_err(),
            TeamError::DifferentIdentity
        );
        assert_eq!(team.status().unwrap().unwrap().id, original.id);
        assert_eq!(team.member_key().unwrap(), "nx_synthetic_fixture");
        team.disconnect().await.unwrap();
        let changed = team
            .connect(&gateway, "nx_new_synthetic_fixture", &cancel)
            .await
            .unwrap();
        assert_ne!(changed.id, original.id);
        server.abort();
    }

    #[tokio::test]
    async fn failed_metadata_commit_restores_the_previous_credential() {
        let profile = Arc::new(Mutex::new(fixture_profile()));
        let response = profile.clone();
        let (gateway, server) = api::tests::serve(Router::new().route(
            "/api/v1/me/team-profile",
            get(move || async move {
                Json(serde_json::json!({"code":0,"data":response.lock().unwrap().clone()}))
            }),
        ))
        .await;
        profile.lock().unwrap().gateway_url = gateway.clone();
        let directory = tempfile::tempdir().unwrap();
        let credentials = Arc::new(MemoryCredentialStore::default());
        let team = TeamService::with_credentials(directory.path(), credentials.clone()).unwrap();
        let cancel = Cancellation::default();
        let first = team
            .connect(&gateway, "nx_previous_synthetic_key", &cancel)
            .await
            .unwrap();
        let database = rusqlite::Connection::open(directory.path().join("team.db")).unwrap();
        database.execute_batch("CREATE TRIGGER fail_profile_save BEFORE INSERT ON team_connection BEGIN SELECT RAISE(FAIL, 'synthetic metadata failure'); END;").unwrap();
        assert_eq!(
            team.connect(&gateway, "nx_replacement_synthetic_key", &cancel)
                .await
                .unwrap_err(),
            TeamError::Storage
        );
        assert_eq!(team.status().unwrap().unwrap().id, first.id);
        assert_eq!(team.member_key().unwrap(), "nx_previous_synthetic_key");
        database
            .execute_batch("DROP TRIGGER fail_profile_save")
            .unwrap();
        team.disconnect().await.unwrap();
        assert!(credentials.0.lock().unwrap().is_empty());
        server.abort();
    }

    #[test]
    #[serial_test::serial]
    fn reopening_team_state_recovers_an_interrupted_install() {
        let temp = tempfile::tempdir().unwrap();
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", temp.path());
        let data_dir = temp.path().join("team-state");
        let credentials = Arc::new(MemoryCredentialStore::default());
        let team = TeamService::with_credentials(&data_dir, credentials.clone()).unwrap();
        let profile = fixture_profile();
        let gateway = "https://gateway.example/";
        let id = connection_id(gateway, &profile).unwrap();
        let connection = TeamConnection {
            id: id.clone(),
            gateway_url: gateway.into(),
            profile,
            status: ConnectionStatus::Connected,
            last_checked_at: Utc::now().to_rfc3339(),
            last_error: None,
        };
        team.state.save_connection(&connection).unwrap();
        let root = crate::config::get_app_config_dir()
            .join("team")
            .join("managed")
            .join(id.chars().take(12).collect::<String>())
            .join("codex");
        std::fs::create_dir_all(&root).unwrap();
        let body = b"recover me";
        let item = ManifestItem {
            asset_id: 99,
            kind: types::AssetKind::Rule,
            slug: "recovery".into(),
            name: "Recovery".into(),
            revision: 1,
            content_hash: format!("{:x}", Sha256::digest(body)),
            archive_sha256: None,
            download_url: "/api/v1/assets/99/revisions/1/download".into(),
            content_type: "text/plain; charset=utf-8".into(),
            byte_size: body.len() as u64,
            files: vec![],
        };
        let relative = std::path::Path::new("rules/asset-99.md");
        let receipt = install::install_verified(
            &item,
            body,
            &root,
            relative,
            None,
            install::InstallPolicy::default(),
        )
        .unwrap();
        team.state
            .save_asset_state(&ManagedAssetState {
                connection_id: id,
                app: "codex".into(),
                asset_id: item.asset_id,
                asset_name: item.name.clone(),
                asset_kind: Some(item.kind.clone()),
                revision: item.revision,
                content_hash: item.content_hash.clone(),
                archive_sha256: None,
                asset_files: Vec::new(),
                local_hash: receipt.local_hash,
                install_path: receipt.install_path.to_string_lossy().into_owned(),
                install_root: root.to_string_lossy().into_owned(),
                relative_path: relative.to_string_lossy().into_owned(),
                backup_path: None,
                backup_root_path: None,
                backup_created_at: None,
                install_operation: receipt.operation.clone(),
                upstream_pending: false,
                upstream_operation: None,
                upstream_backup_path: None,
                upstream_fingerprint: None,
                shared_upstream_fingerprint: None,
                pending_previous_upstream_fingerprint: None,
                pending_target_upstream_fingerprint: None,
                restored_unmanaged: false,
                pending_disk_state: None,
                pending_limits: None,
                subscribed: true,
                last_synced_at: None,
            })
            .unwrap();
        install::leave_interrupted_backup_for_test(&root, relative).unwrap();
        assert!(!root.join(relative).exists());
        drop(team);

        let reopened = TeamService::with_credentials(&data_dir, credentials).unwrap();
        assert_eq!(std::fs::read(root.join(relative)).unwrap(), body);
        assert!(reopened.status().unwrap().is_some());
        std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
    }

    #[tokio::test]
    #[ignore = "requires the explicit isolated NexusOps HTTP fixture"]
    async fn real_gateway_connect_refresh_and_verified_download() {
        let path = std::env::var_os("NEXUSOPS_TEAM_HTTP_STATE_FILE")
            .expect("set the isolated HTTP fixture state file");
        // Explicit opt-in synthetic fixture; never print its member credential.
        let fixture: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let key = fixture["key"]["key"].as_str().unwrap();
        let gateway = fixture["gateway_url"]
            .as_str()
            .unwrap_or("http://127.0.0.1:18191");
        let directory = tempfile::tempdir().unwrap();
        let team = TeamService::with_credentials(
            directory.path(),
            Arc::new(MemoryCredentialStore::default()),
        )
        .unwrap();
        let cancel = Cancellation::default();
        let connected = team.connect(gateway, key, &cancel).await.unwrap();
        assert_eq!(connected.profile.organization_id, "local");
        assert_eq!(connected.profile.key.platform, "openai");
        assert!(connected.profile.models.is_empty());
        let (_, manifest) = team.refresh(&cancel).await.unwrap();
        assert!(manifest.assets.len() >= 5);
        for item in &manifest.assets {
            let body = team.download(item, &cancel).await.unwrap();
            assert_eq!(body.len() as u64, item.byte_size);
        }
        team.disconnect().await.unwrap();
        assert!(team.status().unwrap().is_none());
    }
}
