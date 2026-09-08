use std::str::FromStr;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::{
    deeplink::{build_provider_from_request, DeepLinkImportRequest},
    error::AppError,
    provider::Provider,
    services::ProviderService,
    store::AppState,
    AppType,
};

use super::{
    types::{ConnectionStatus, ManagedProviderLink, TeamConnection, TeamError, TeamProfile},
    TeamService,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderChange {
    Create,
    Unchanged,
    Recreate,
    UpdateRequiresConfirmation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderPreview {
    pub app: String,
    pub provider_id: String,
    pub name: String,
    pub base_url: String,
    pub authorized_models: Vec<String>,
    pub selected_model: Option<String>,
    pub active: bool,
    pub change: ProviderChange,
    pub changed_fields: Vec<String>,
    pub decision_token: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TeamProviderError {
    #[error(transparent)]
    Team(#[from] TeamError),
    #[error("unsupported Team provider app: {0}")]
    UnsupportedApp(String),
    #[error("the selected model is not authorized by this Team profile")]
    UnauthorizedModel,
    #[error("the generated Team provider id is already used by a personal provider")]
    ProviderCollision,
    #[error("review and confirm the Team provider changes before applying them")]
    ConfirmationRequired,
    #[error("the Team provider changed after preview; review it again")]
    PreviewStale,
    #[error("switch away from the active Team provider before updating it")]
    ActiveProviderUpdate,
    #[error("the Team provider link is missing")]
    MissingLink,
    #[error("the Team provider link is invalid")]
    InvalidLink,
    #[error("switch to a personal or official provider before removing Team credentials")]
    NoFallbackProvider,
    #[error("provider operation failed: {0}")]
    Provider(String),
}

impl From<AppError> for TeamProviderError {
    fn from(value: AppError) -> Self {
        Self::Provider(value.to_string())
    }
}

impl TeamProviderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Team(error) => error.code(),
            Self::UnsupportedApp(_) => "unsupported_app",
            Self::UnauthorizedModel => "unauthorized_model",
            Self::ProviderCollision => "provider_collision",
            Self::ConfirmationRequired => "confirmation_required",
            Self::PreviewStale => "provider_preview_stale",
            Self::ActiveProviderUpdate => "active_provider_update",
            Self::MissingLink => "missing_provider_link",
            Self::InvalidLink => "invalid_provider_link",
            Self::NoFallbackProvider => "no_fallback_provider",
            Self::Provider(_) => "provider_error",
        }
    }
}

pub fn preview_provider(
    team: &TeamService,
    state: &AppState,
    app: &str,
    model: Option<&str>,
) -> Result<ProviderPreview, TeamProviderError> {
    let connection = connected_team(team)?;
    let app_type = supported_app(app, &connection.profile)?;
    let mut desired = build_team_provider(&connection, &app_type, model, &team.member_key()?)?;
    use_linked_id(team, &connection, &app_type, &mut desired)?;
    let desired = ProviderService::prepare_inactive(state, &app_type, desired)?;
    preview_for(&connection, team, state, &app_type, model, &desired)
}

pub fn apply_reviewed_provider(
    team: &TeamService,
    state: &AppState,
    app: &str,
    model: Option<&str>,
    confirm_update: bool,
    decision_token: Option<&str>,
) -> Result<ProviderPreview, TeamProviderError> {
    let current = preview_provider(team, state, app, model)?;
    if decision_token != Some(current.decision_token.as_str()) {
        return Err(TeamProviderError::PreviewStale);
    }
    apply_provider(team, state, app, model, confirm_update)
}

fn apply_provider(
    team: &TeamService,
    state: &AppState,
    app: &str,
    model: Option<&str>,
    confirm_update: bool,
) -> Result<ProviderPreview, TeamProviderError> {
    let connection = connected_team(team)?;
    let app_type = supported_app(app, &connection.profile)?;
    let mut desired = build_team_provider(&connection, &app_type, model, &team.member_key()?)?;
    use_linked_id(team, &connection, &app_type, &mut desired)?;
    let desired = ProviderService::prepare_inactive(state, &app_type, desired)?;
    let preview = preview_for(&connection, team, state, &app_type, model, &desired)?;
    if preview.change == ProviderChange::UpdateRequiresConfirmation && !confirm_update {
        return Err(TeamProviderError::ConfirmationRequired);
    }

    let existing = state
        .db
        .get_provider_by_id(&desired.id, app_type.as_str())?;
    let existing_link = team
        .state
        .provider_links(&connection.id)?
        .into_iter()
        .find(|link| link.app == app_type.as_str());
    let mut provider_to_save = desired.clone();
    if let Some(existing) = existing.as_ref() {
        preserve_personal_fields(
            &app_type,
            existing,
            &mut provider_to_save,
            existing_link
                .as_ref()
                .and_then(|link| link.managed_model.as_deref()),
        );
    }
    let current = state.db.get_current_provider(app_type.as_str())?;
    let active = current.as_deref() == Some(provider_to_save.id.as_str())
        || crate::settings::get_current_provider(&app_type).as_deref()
            == Some(provider_to_save.id.as_str())
        || (app_type.is_additive_mode()
            && existing.as_ref().is_some_and(|provider| {
                ProviderService::provider_live_config_managed(provider) == Some(true)
            }));
    if active && preview.change != ProviderChange::Unchanged {
        return Err(TeamProviderError::ActiveProviderUpdate);
    }
    let next_link = ManagedProviderLink {
        connection_id: connection.id.clone(),
        app: app_type.as_str().to_owned(),
        provider_id: provider_to_save.id.clone(),
        managed_model: model.map(str::to_owned),
    };
    if existing.is_none() {
        // Persist ownership before creating the provider. A crash between the
        // two writes is then a recoverable Recreate, never a personal collision.
        team.state.save_provider_link(&next_link)?;
    }
    if !active && preview.change != ProviderChange::Unchanged {
        ProviderService::upsert_inactive(state, app_type.clone(), provider_to_save.clone())?;
    }
    if existing.is_some() || preview.change == ProviderChange::Unchanged {
        // For updates, keep the previous managed model until the provider is
        // durable so a retry can still remove only the old Team-owned alias.
        team.state.save_provider_link(&next_link)?;
    }

    preview_provider(team, state, app_type.as_str(), model)
}

pub fn activate_reviewed_provider(
    team: &TeamService,
    state: &AppState,
    app: &str,
    decision_token: Option<&str>,
) -> Result<(), TeamProviderError> {
    let connection = connected_team(team)?;
    let link = team
        .state
        .provider_links(&connection.id)?
        .into_iter()
        .find(|link| link.app == app)
        .ok_or(TeamProviderError::MissingLink)?;
    let current = preview_provider(team, state, app, link.managed_model.as_deref())?;
    if current.change != ProviderChange::Unchanged
        || decision_token != Some(current.decision_token.as_str())
    {
        return Err(TeamProviderError::PreviewStale);
    }
    activate_provider(team, state, app)
}

fn activate_provider(
    team: &TeamService,
    state: &AppState,
    app: &str,
) -> Result<(), TeamProviderError> {
    let connection = connected_team(team)?;
    let app_type = supported_app(app, &connection.profile)?;
    let link = team
        .state
        .provider_links(&connection.id)?
        .into_iter()
        .find(|link| link.app == app_type.as_str())
        .ok_or(TeamProviderError::MissingLink)?;
    ProviderService::switch(state, app_type, &link.provider_id)?;
    Ok(())
}

pub fn scrub_provider_credentials(
    team: &TeamService,
    state: &AppState,
) -> Result<usize, TeamProviderError> {
    let connection = team.status()?.ok_or(TeamError::NotConnected)?;
    struct ScrubPlan {
        app: AppType,
        provider_id: String,
        provider: Option<Provider>,
        remove_from_live: bool,
        fallback: Option<String>,
    }

    // Validate the complete batch before changing any provider. In particular,
    // a missing fallback in one app must not leave earlier apps already scrubbed.
    let mut plan = Vec::new();
    for link in team.state.provider_links(&connection.id)? {
        let app_type = AppType::from_str(&link.app).map_err(|_| TeamProviderError::InvalidLink)?;
        if !matches!(
            app_type,
            AppType::Claude
                | AppType::Codex
                | AppType::Gemini
                | AppType::GrokBuild
                | AppType::OpenCode
        ) || link.provider_id != stable_provider_id(&connection.id, &app_type)
        {
            return Err(TeamProviderError::InvalidLink);
        }
        let provider = state
            .db
            .get_provider_by_id(&link.provider_id, app_type.as_str())?;
        let remove_from_live = provider.as_ref().is_some_and(|provider| {
            app_type.is_additive_mode()
                && ProviderService::provider_live_config_managed(provider) == Some(true)
        });
        let active = !app_type.is_additive_mode()
            && (state.db.get_current_provider(app_type.as_str())?.as_deref()
                == Some(link.provider_id.as_str())
                || crate::settings::get_current_provider(&app_type).as_deref()
                    == Some(link.provider_id.as_str()));
        let fallback = if active {
            let providers = state.db.get_all_providers(app_type.as_str())?;
            Some(
                providers
                    .values()
                    .filter(|provider| {
                        provider.id != link.provider_id
                            && !provider.id.starts_with("nexusops-team-")
                    })
                    .min_by_key(|provider| {
                        if provider.category.as_deref() == Some("official") {
                            0
                        } else {
                            1
                        }
                    })
                    .ok_or(TeamProviderError::NoFallbackProvider)?
                    .id
                    .clone(),
            )
        } else {
            None
        };
        plan.push(ScrubPlan {
            app: app_type,
            provider_id: link.provider_id,
            provider,
            remove_from_live,
            fallback,
        });
    }

    let mut removed = 0usize;
    for item in plan {
        let Some(_provider) = item.provider else {
            continue;
        };
        if item.remove_from_live {
            ProviderService::remove_from_live_config(state, item.app.clone(), &item.provider_id)?;
        }
        if let Some(fallback) = item.fallback.as_deref() {
            ProviderService::switch(state, item.app.clone(), fallback)?;
        }
        ProviderService::delete(state, item.app, &item.provider_id)?;
        removed += 1;
    }
    Ok(removed)
}

fn connected_team(team: &TeamService) -> Result<TeamConnection, TeamProviderError> {
    let connection = team.status()?.ok_or(TeamError::NotConnected)?;
    match connection.status {
        ConnectionStatus::Connected => Ok(connection),
        ConnectionStatus::AuthenticationRequired => Err(TeamError::AuthenticationRequired.into()),
        ConnectionStatus::AccessDenied => Err(TeamError::AccessDenied.into()),
        ConnectionStatus::Unavailable => Err(TeamError::Unavailable.into()),
    }
}

fn preview_for(
    connection: &TeamConnection,
    team: &TeamService,
    state: &AppState,
    app_type: &AppType,
    model: Option<&str>,
    desired: &Provider,
) -> Result<ProviderPreview, TeamProviderError> {
    let link = team
        .state
        .provider_links(&connection.id)?
        .into_iter()
        .find(|link| link.app == app_type.as_str());
    let provider_id = link
        .as_ref()
        .map(|link| link.provider_id.as_str())
        .unwrap_or(desired.id.as_str());
    let existing = state
        .db
        .get_provider_by_id(provider_id, app_type.as_str())?;
    if link.is_none() && existing.is_some() {
        return Err(TeamProviderError::ProviderCollision);
    }

    let changed_fields = existing
        .as_ref()
        .map(|provider| {
            managed_changes(
                app_type,
                provider,
                desired,
                link.as_ref().and_then(|link| link.managed_model.as_deref()),
            )
        })
        .unwrap_or_default();
    let change = match (&link, &existing, changed_fields.is_empty()) {
        (None, None, _) => ProviderChange::Create,
        (Some(_), None, _) => ProviderChange::Recreate,
        (Some(_), Some(_), true) => ProviderChange::Unchanged,
        (Some(_), Some(_), false) => ProviderChange::UpdateRequiresConfirmation,
        (None, Some(_), _) => unreachable!("collision returned above"),
    };
    let current = state.db.get_current_provider(app_type.as_str())?;
    let local_current = crate::settings::get_current_provider(app_type);
    let active = existing.as_ref().is_some_and(|provider| {
        current.as_deref() == Some(provider.id.as_str())
            || local_current.as_deref() == Some(provider.id.as_str())
            || (app_type.is_additive_mode()
                && ProviderService::provider_live_config_managed(provider) == Some(true))
    });

    // Bind confirmation to both the intended configuration and the local state.
    // The per-connection in-memory MAC key prevents the token from exposing a
    // reusable credential digest, and invalidates previews across reconnects.
    let secret = team
        .provider_review_secret
        .lock()
        .map_err(|_| TeamError::Storage)?;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| TeamError::Storage)?;
    let mut snapshot = serde_json::to_value((
        &connection.id,
        &connection.profile,
        app_type.as_str(),
        model,
        &link,
        &existing,
        desired,
        &current,
        &local_current,
    ))
    .map_err(|_| TeamError::Storage)?;
    // Provider metadata contains HashMaps whose iteration order changes on reload.
    snapshot.sort_all_objects();
    mac.update(&serde_json::to_vec(&snapshot).map_err(|_| TeamError::Storage)?);
    let decision_token = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    Ok(ProviderPreview {
        app: app_type.as_str().to_owned(),
        provider_id: provider_id.to_owned(),
        name: desired.name.clone(),
        base_url: provider_endpoint(&connection.profile, app_type),
        authorized_models: connection
            .profile
            .models
            .iter()
            .map(|model| model.id.clone())
            .collect(),
        selected_model: model.map(str::to_owned),
        active,
        change,
        changed_fields,
        decision_token,
    })
}

fn supported_app(value: &str, profile: &TeamProfile) -> Result<AppType, TeamProviderError> {
    let app = AppType::from_str(value)
        .map_err(|_| TeamProviderError::UnsupportedApp(value.to_owned()))?;
    let platform = profile.key.platform.trim().to_ascii_lowercase();
    let supported = match app {
        AppType::Claude => {
            matches!(platform.as_str(), "anthropic" | "antigravity" | "grok")
                || (platform == "openai" && profile.key.allow_messages_dispatch)
        }
        AppType::Codex => matches!(platform.as_str(), "openai" | "grok"),
        AppType::Gemini => matches!(platform.as_str(), "gemini" | "antigravity"),
        AppType::GrokBuild => platform == "grok",
        AppType::OpenCode => false,
        _ => false,
    };
    if supported {
        Ok(app)
    } else {
        Err(TeamProviderError::UnsupportedApp(value.to_owned()))
    }
}

fn build_team_provider(
    connection: &TeamConnection,
    app_type: &AppType,
    model: Option<&str>,
    key: &str,
) -> Result<Provider, TeamProviderError> {
    let model = model.ok_or(TeamProviderError::UnauthorizedModel)?;
    if !connection
        .profile
        .models
        .iter()
        .any(|candidate| candidate.id == model)
    {
        return Err(TeamProviderError::UnauthorizedModel);
    }
    let request = DeepLinkImportRequest {
        version: "v1".into(),
        resource: "provider".into(),
        app: Some(app_type.as_str().into()),
        name: Some(format!("NexusOps · {}", connection.profile.name)),
        enabled: Some(false),
        homepage: Some(connection.profile.gateway_url.clone()),
        endpoint: Some(provider_endpoint(&connection.profile, app_type)),
        api_key: Some(key.to_owned()),
        icon: Some(icon_for(app_type).into()),
        model: Some(model.to_owned()),
        haiku_model: (connection.profile.key.platform.eq_ignore_ascii_case("grok")
            && *app_type == AppType::Claude)
            .then(|| model.to_owned()),
        sonnet_model: (connection.profile.key.platform.eq_ignore_ascii_case("grok")
            && *app_type == AppType::Claude)
            .then(|| model.to_owned()),
        opus_model: (connection.profile.key.platform.eq_ignore_ascii_case("grok")
            && *app_type == AppType::Claude)
            .then(|| model.to_owned()),
        ..DeepLinkImportRequest::default()
    };
    let mut provider = build_provider_from_request(app_type, &request)?;
    provider.id = stable_provider_id(&connection.id, app_type);
    Ok(provider)
}

fn provider_endpoint(profile: &TeamProfile, app: &AppType) -> String {
    let base = profile.base_url.trim().trim_end_matches('/');
    let root = base.strip_suffix("/v1").unwrap_or(base);
    let platform = profile.key.platform.to_ascii_lowercase();
    match (platform.as_str(), app) {
        ("antigravity", AppType::Claude | AppType::Gemini) => {
            format!("{root}/antigravity")
        }
        ("grok", AppType::Codex | AppType::GrokBuild | AppType::OpenCode) => {
            format!("{root}/v1")
        }
        (_, AppType::Claude | AppType::Gemini) => root.into(),
        _ => base.into(),
    }
}

fn stable_provider_id(connection_id: &str, app_type: &AppType) -> String {
    let prefix = connection_id.chars().take(12).collect::<String>();
    format!("nexusops-team-{prefix}-{}", app_type.as_str())
}

fn use_linked_id(
    team: &TeamService,
    connection: &TeamConnection,
    app_type: &AppType,
    provider: &mut Provider,
) -> Result<(), TeamProviderError> {
    if let Some(link) = team
        .state
        .provider_links(&connection.id)?
        .into_iter()
        .find(|link| link.app == app_type.as_str())
    {
        if link.provider_id != stable_provider_id(&connection.id, app_type) {
            return Err(TeamProviderError::InvalidLink);
        }
        provider.id = link.provider_id;
    }
    Ok(())
}

fn icon_for(app_type: &AppType) -> &'static str {
    match app_type {
        AppType::Claude => "anthropic",
        AppType::Codex => "openai",
        AppType::Gemini => "gemini",
        AppType::GrokBuild => "grok",
        AppType::OpenCode => "opencode",
        _ => "nexusops",
    }
}

fn managed_changes(
    app: &AppType,
    existing: &Provider,
    desired: &Provider,
    previous_managed_model: Option<&str>,
) -> Vec<String> {
    let mut changes = Vec::new();
    if existing.name != desired.name {
        changes.push("name".into());
    }
    let mut merged = desired.clone();
    preserve_personal_fields(app, existing, &mut merged, previous_managed_model);
    if normalized_settings(&existing.settings_config)
        != normalized_settings(&merged.settings_config)
    {
        changes.push("settings".into());
    }
    if existing.website_url != desired.website_url {
        changes.push("base_url".into());
    }
    if existing.icon != desired.icon {
        changes.push("icon".into());
    }
    changes
}

fn preserve_personal_fields(
    app: &AppType,
    existing: &Provider,
    desired: &mut Provider,
    previous_managed_model: Option<&str>,
) {
    desired.settings_config =
        merge_managed_settings(app, existing, desired, previous_managed_model);
    desired.category = existing.category.clone();
    desired.created_at = existing.created_at;
    desired.sort_index = existing.sort_index;
    desired.notes = existing.notes.clone();
    desired.meta = existing.meta.clone();
    desired.icon_color = existing.icon_color.clone();
    desired.in_failover_queue = existing.in_failover_queue;
}

fn merge_managed_settings(
    app: &AppType,
    existing: &Provider,
    desired: &Provider,
    previous_managed_model: Option<&str>,
) -> serde_json::Value {
    let mut merged = existing.settings_config.clone();
    match app {
        AppType::Claude | AppType::Gemini => {
            let Some(target_env) = desired
                .settings_config
                .get("env")
                .and_then(|value| value.as_object())
            else {
                return desired.settings_config.clone();
            };
            let Some(root) = merged.as_object_mut() else {
                return desired.settings_config.clone();
            };
            let env = root.entry("env").or_insert_with(|| serde_json::json!({}));
            let Some(env) = env.as_object_mut() else {
                return desired.settings_config.clone();
            };
            let owned_keys: &[&str] = match app {
                AppType::Claude => &[
                    "ANTHROPIC_AUTH_TOKEN",
                    "ANTHROPIC_API_KEY",
                    "ANTHROPIC_BASE_URL",
                    "ANTHROPIC_MODEL",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL",
                ],
                AppType::Gemini => &["GEMINI_API_KEY", "GOOGLE_GEMINI_BASE_URL", "GEMINI_MODEL"],
                _ => &[],
            };
            for key in owned_keys {
                env.remove(*key);
            }
            for (key, value) in target_env {
                env.insert(key.clone(), value.clone());
            }
        }
        AppType::Codex | AppType::GrokBuild => {
            if let (Some(existing_auth), Some(desired_auth)) = (
                merged
                    .get_mut("auth")
                    .and_then(|value| value.as_object_mut()),
                desired
                    .settings_config
                    .get("auth")
                    .and_then(|value| value.as_object()),
            ) {
                for (key, value) in desired_auth {
                    existing_auth.insert(key.clone(), value.clone());
                }
            } else if let Some(auth) = desired.settings_config.get("auth") {
                if let Some(root) = merged.as_object_mut() {
                    root.insert("auth".into(), auth.clone());
                }
            }
            let current_config = existing
                .settings_config
                .get("config")
                .and_then(|value| value.as_str());
            let desired_config = desired
                .settings_config
                .get("config")
                .and_then(|value| value.as_str());
            if let (Some(current), Some(target)) = (current_config, desired_config) {
                if let Some(config) =
                    merge_managed_toml(app, current, target, previous_managed_model)
                {
                    if let Some(root) = merged.as_object_mut() {
                        root.insert("config".into(), serde_json::Value::String(config));
                    }
                } else {
                    return desired.settings_config.clone();
                }
            }
        }
        _ => return desired.settings_config.clone(),
    }
    merged
}

fn merge_managed_toml(
    app: &AppType,
    current: &str,
    target: &str,
    previous_managed_model: Option<&str>,
) -> Option<String> {
    let mut current: toml::Value = toml::from_str(current).ok()?;
    let target: toml::Value = toml::from_str(target).ok()?;
    let current_root = current.as_table_mut()?;
    let target_root = target.as_table()?;
    match app {
        AppType::Codex => {
            copy_toml_keys(current_root, target_root, &["model_provider", "model"]);
            let provider = target_root.get("model_provider")?.as_str()?;
            let desired_provider = target_root
                .get("model_providers")?
                .as_table()?
                .get(provider)?
                .as_table()?;
            let providers = current_root
                .entry("model_providers")
                .or_insert_with(|| toml::Value::Table(Default::default()))
                .as_table_mut()?;
            let current_provider = providers
                .entry(provider)
                .or_insert_with(|| toml::Value::Table(Default::default()))
                .as_table_mut()?;
            copy_toml_keys(
                current_provider,
                desired_provider,
                &["name", "base_url", "wire_api", "requires_openai_auth"],
            );
        }
        AppType::GrokBuild => {
            let desired_models = target_root.get("models")?.as_table()?;
            let model = desired_models.get("default")?.as_str()?;
            let models = current_root
                .entry("models")
                .or_insert_with(|| toml::Value::Table(Default::default()))
                .as_table_mut()?;
            copy_toml_keys(models, desired_models, &["default"]);
            let desired_model = target_root
                .get("model")?
                .as_table()?
                .get(model)?
                .as_table()?;
            let model_table = current_root
                .entry("model")
                .or_insert_with(|| toml::Value::Table(Default::default()))
                .as_table_mut()?;
            if let Some(previous) = previous_managed_model {
                if previous != model {
                    let remove_previous = model_table
                        .get_mut(previous)
                        .and_then(toml::Value::as_table_mut)
                        .map(|table| {
                            for key in [
                                "model",
                                "base_url",
                                "name",
                                "api_key",
                                "api_backend",
                                "context_window",
                            ] {
                                table.remove(key);
                            }
                            table.is_empty()
                        })
                        .unwrap_or(false);
                    if remove_previous {
                        model_table.remove(previous);
                    }
                }
            }
            let current_model = model_table
                .entry(model)
                .or_insert_with(|| toml::Value::Table(Default::default()))
                .as_table_mut()?;
            copy_toml_keys(
                current_model,
                desired_model,
                &[
                    "model",
                    "base_url",
                    "name",
                    "api_key",
                    "api_backend",
                    "context_window",
                ],
            );
        }
        _ => return None,
    }
    toml::to_string(&current).ok()
}

fn copy_toml_keys(current: &mut toml::value::Table, target: &toml::value::Table, keys: &[&str]) {
    for key in keys {
        if let Some(value) = target.get(*key) {
            current.insert((*key).into(), value.clone());
        }
    }
}

fn normalized_settings(settings: &serde_json::Value) -> serde_json::Value {
    let mut normalized = settings.clone();
    if let Some(config) = normalized
        .get("config")
        .and_then(|value| value.as_str())
        .and_then(|value| toml::from_str::<toml::Value>(value).ok())
        .and_then(|value| serde_json::to_value(value).ok())
    {
        if let Some(root) = normalized.as_object_mut() {
            root.insert("config".into(), config);
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        database::Database,
        services::team::{
            credentials::{fixtures::MemoryCredentialStore, CredentialStore},
            types::{fixture_profile, TeamProfile},
        },
    };
    use serial_test::serial;

    fn connection() -> TeamConnection {
        let mut profile: TeamProfile = fixture_profile();
        profile.models = vec![crate::services::team::types::ProfileModel {
            id: "approved-model".into(),
            name: "Approved model".into(),
            platform: "openai".into(),
        }];
        TeamConnection {
            id: "0123456789abcdef".into(),
            gateway_url: "https://gateway.example/".into(),
            profile,
            status: super::super::types::ConnectionStatus::Connected,
            last_checked_at: "2026-09-07T00:00:00Z".into(),
            last_error: None,
        }
    }

    #[test]
    fn provider_id_is_stable_and_only_authorized_model_is_accepted() {
        let connection = connection();
        let provider = build_team_provider(
            &connection,
            &AppType::Codex,
            Some("approved-model"),
            "nx_private_fixture",
        )
        .unwrap();
        assert_eq!(provider.id, "nexusops-team-0123456789ab-codex");
        assert!(provider
            .settings_config
            .to_string()
            .contains("approved-model"));
        assert!(matches!(
            build_team_provider(
                &connection,
                &AppType::Codex,
                Some("not-authorized"),
                "nx_private_fixture"
            ),
            Err(TeamProviderError::UnauthorizedModel)
        ));
        assert!(matches!(
            build_team_provider(&connection, &AppType::Codex, None, "nx_private_fixture"),
            Err(TeamProviderError::UnauthorizedModel)
        ));
        assert!(matches!(
            supported_app("claude", &connection.profile),
            Err(TeamProviderError::UnsupportedApp(_))
        ));
        let mut dispatch = connection.profile.clone();
        dispatch.key.allow_messages_dispatch = true;
        assert_eq!(supported_app("claude", &dispatch).unwrap(), AppType::Claude);

        let mut antigravity = connection.profile.clone();
        antigravity.key.platform = "antigravity".into();
        antigravity.base_url = "https://gateway.example/prefix/v1".into();
        assert_eq!(
            provider_endpoint(&antigravity, &AppType::Claude),
            "https://gateway.example/prefix/antigravity"
        );
        let mut grok = connection.profile.clone();
        grok.key.platform = "grok".into();
        grok.base_url = "https://gateway.example/prefix/v1".into();
        assert_eq!(
            provider_endpoint(&grok, &AppType::Claude),
            "https://gateway.example/prefix"
        );
    }

    #[tokio::test]
    async fn generated_tool_settings_reach_gateway_protocol_routes() {
        use axum::{http::StatusCode, routing::post, Router};

        // These are the Gateway's registered protocol paths behind /proxy.
        let router = Router::new()
            .route(
                "/proxy/v1/messages",
                post(|| async { StatusCode::NO_CONTENT }),
            )
            .route(
                "/proxy/v1beta/models/approved-model:generateContent",
                post(|| async { StatusCode::NO_CONTENT }),
            )
            .route(
                "/proxy/antigravity/v1/messages",
                post(|| async { StatusCode::NO_CONTENT }),
            )
            .route(
                "/proxy/antigravity/v1beta/models/approved-model:generateContent",
                post(|| async { StatusCode::NO_CONTENT }),
            );
        let (origin, server) = super::super::api::tests::serve(router).await;
        let client = reqwest::Client::new();
        for (platform, app, variable, path) in [
            (
                "anthropic",
                AppType::Claude,
                "ANTHROPIC_BASE_URL",
                "v1/messages",
            ),
            (
                "openai",
                AppType::Claude,
                "ANTHROPIC_BASE_URL",
                "v1/messages",
            ),
            (
                "gemini",
                AppType::Gemini,
                "GOOGLE_GEMINI_BASE_URL",
                "v1beta/models/approved-model:generateContent",
            ),
            (
                "antigravity",
                AppType::Claude,
                "ANTHROPIC_BASE_URL",
                "v1/messages",
            ),
            (
                "antigravity",
                AppType::Gemini,
                "GOOGLE_GEMINI_BASE_URL",
                "v1beta/models/approved-model:generateContent",
            ),
        ] {
            let mut connection = connection();
            connection.profile.key.platform = platform.into();
            connection.profile.key.allow_messages_dispatch = true;
            for suffix in ["", "/v1", "/v1///"] {
                connection.profile.base_url = format!("{origin}proxy{suffix}");
                let provider = build_team_provider(
                    &connection,
                    &app,
                    Some("approved-model"),
                    "nx_synthetic_fixture",
                )
                .unwrap();
                let base = provider.settings_config["env"][variable].as_str().unwrap();
                let response = client.post(format!("{base}/{path}")).send().await.unwrap();
                assert_eq!(
                    response.status(),
                    reqwest::StatusCode::NO_CONTENT,
                    "{platform} / {app:?} / {suffix}"
                );
            }
        }
        server.abort();
    }

    #[test]
    fn tool_endpoints_preserve_non_version_prefixes_and_codex_configuration() {
        let mut profile = connection().profile;
        profile.base_url = "https://gateway.example/proxy/v10/".into();
        assert_eq!(
            provider_endpoint(&profile, &AppType::Codex),
            "https://gateway.example/proxy/v10"
        );
        assert_eq!(
            provider_endpoint(&profile, &AppType::Claude),
            "https://gateway.example/proxy/v10"
        );
        profile.key.platform = "antigravity".into();
        assert_eq!(
            provider_endpoint(&profile, &AppType::Gemini),
            "https://gateway.example/proxy/v10/antigravity"
        );
    }

    #[tokio::test]
    #[serial]
    #[ignore = "requires the isolated Gateway fixture with synthetic core_tools accounts"]
    async fn real_gateway_provider_import_activation_and_model_request() {
        let path =
            std::env::var_os("NEXUSOPS_TEAM_HTTP_STATE_FILE").expect("set isolated fixture state");
        let fixture: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let gateway = fixture["gateway_url"].as_str().unwrap();
        assert!(gateway.starts_with("http://127.0.0.1:"));
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", root.path());
        crate::settings::reload_settings().unwrap();
        for (platform, app, base_variable, auth_variable) in [
            (
                "anthropic",
                AppType::Claude,
                "ANTHROPIC_BASE_URL",
                "ANTHROPIC_AUTH_TOKEN",
            ),
            (
                "gemini",
                AppType::Gemini,
                "GOOGLE_GEMINI_BASE_URL",
                "GEMINI_API_KEY",
            ),
        ] {
            let key = fixture["core_tools"][platform]["key"].as_str().unwrap();
            let model = fixture["core_tools"][platform]["model"].as_str().unwrap();
            let credentials = Arc::new(MemoryCredentialStore::default());
            let team =
                TeamService::with_credentials(&root.path().join(platform), credentials).unwrap();
            let state = AppState::new(Arc::new(Database::memory().unwrap()));
            let cancel = super::super::api::Cancellation::default();
            team.connect(gateway, key, &cancel).await.unwrap();
            team.refresh(&cancel).await.unwrap();
            let preview = preview_provider(&team, &state, app.as_str(), Some(model)).unwrap();
            assert_eq!(preview.change, ProviderChange::Create);
            assert!(preview.authorized_models.iter().any(|id| id == model));
            team.refresh(&cancel).await.unwrap();
            let imported = apply_reviewed_provider(
                &team,
                &state,
                app.as_str(),
                Some(model),
                false,
                Some(&preview.decision_token),
            )
            .unwrap();
            assert!(!imported.active);
            team.refresh(&cancel).await.unwrap();
            let repeated = apply_reviewed_provider(
                &team,
                &state,
                app.as_str(),
                Some(model),
                false,
                Some(&imported.decision_token),
            )
            .unwrap();
            team.refresh(&cancel).await.unwrap();
            activate_reviewed_provider(&team, &state, app.as_str(), Some(&repeated.decision_token))
                .unwrap();
            assert!(
                preview_provider(&team, &state, app.as_str(), Some(model))
                    .unwrap()
                    .active
            );
            let provider = state
                .db
                .get_provider_by_id(&preview.provider_id, app.as_str())
                .unwrap()
                .unwrap();
            let env = &provider.settings_config["env"];
            let base = env[base_variable].as_str().unwrap();
            let saved_key = env[auth_variable].as_str().unwrap();
            assert!(
                saved_key == key,
                "import must preserve the selected member credential"
            );
            let (url, header, body) = if app == AppType::Claude {
                (
                    format!("{base}/v1/messages"),
                    "x-api-key",
                    serde_json::json!({"model":model,"max_tokens":8,"messages":[{"role":"user","content":"Return fixture-ok"}]}),
                )
            } else {
                (
                    format!("{base}/v1beta/models/{model}:generateContent"),
                    "x-goog-api-key",
                    serde_json::json!({"contents":[{"role":"user","parts":[{"text":"Return fixture-ok"}]}]}),
                )
            };
            let response = reqwest::Client::new()
                .post(url)
                .header(header, saved_key)
                .json(&body)
                .send()
                .await
                .unwrap();
            assert!(
                response.status().is_success(),
                "{platform} inference failed"
            );
            assert!(response.text().await.unwrap().contains("fixture-ok"));
            team.disconnect().await.unwrap();
            assert!(team.status().unwrap().is_none());
        }
        std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
    }

    #[test]
    fn preview_shape_never_contains_the_member_key() {
        let preview = ProviderPreview {
            app: "codex".into(),
            provider_id: "nexusops-team-fixture-codex".into(),
            name: "NexusOps Team".into(),
            base_url: "https://gateway.example/api/v1".into(),
            authorized_models: vec!["approved-model".into()],
            selected_model: Some("approved-model".into()),
            active: false,
            change: ProviderChange::Create,
            changed_fields: vec![],
            decision_token: "opaque-review-fixture".into(),
        };
        let json = serde_json::to_string(&preview).unwrap();
        assert!(!json.contains("nx_private_fixture"));
        assert!(!json.contains("api_key"));
    }

    #[test]
    #[serial]
    fn review_rejects_credentials_changed_after_preview() {
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", root.path());
        crate::settings::reload_settings().unwrap();
        let credentials = Arc::new(MemoryCredentialStore::default());
        let team =
            TeamService::with_credentials(&root.path().join("team"), credentials.clone()).unwrap();
        let mut connection = connection();
        connection.id =
            super::super::connection_id(&connection.gateway_url, &connection.profile).unwrap();
        team.state.save_connection(&connection).unwrap();
        credentials
            .set(&connection.id, "nx_original_review_fixture")
            .unwrap();
        let state = AppState::new(Arc::new(Database::memory().unwrap()));
        assert!(matches!(
            apply_reviewed_provider(&team, &state, "codex", Some("approved-model"), false, None),
            Err(TeamProviderError::PreviewStale)
        ));
        assert!(team
            .state
            .provider_links(&connection.id)
            .unwrap()
            .is_empty());
        apply_provider(&team, &state, "codex", Some("approved-model"), false).unwrap();
        credentials
            .set(&connection.id, "nx_preview_review_fixture")
            .unwrap();
        let reviewed = preview_provider(&team, &state, "codex", Some("approved-model")).unwrap();
        credentials
            .set(&connection.id, "nx_changed_after_review_fixture")
            .unwrap();
        let result = apply_reviewed_provider(
            &team,
            &state,
            "codex",
            Some("approved-model"),
            true,
            Some(&reviewed.decision_token),
        );
        assert!(
            matches!(result, Err(TeamProviderError::PreviewStale)),
            "an old confirmation must not apply a newly rotated credential"
        );
        let current = preview_provider(&team, &state, "codex", Some("approved-model")).unwrap();
        let reopened =
            TeamService::with_credentials(&root.path().join("team"), credentials.clone()).unwrap();
        assert!(matches!(
            apply_reviewed_provider(
                &reopened,
                &state,
                "codex",
                Some("approved-model"),
                true,
                Some(&current.decision_token)
            ),
            Err(TeamProviderError::PreviewStale)
        ));
        let imported = apply_reviewed_provider(
            &team,
            &state,
            "codex",
            Some("approved-model"),
            true,
            Some(&current.decision_token),
        )
        .unwrap();
        assert_eq!(imported.change, ProviderChange::Unchanged);
        assert!(!serde_json::to_string(&imported)
            .unwrap()
            .contains("nx_changed_after_review_fixture"));
        let mut edited = state
            .db
            .get_provider_by_id(&imported.provider_id, "codex")
            .unwrap()
            .unwrap();
        edited.notes = Some("Personal note edited after preview".into());
        edited.meta = Some(
            serde_json::from_value(serde_json::json!({
                "custom_endpoints": {
                    "https://a.example": {"url": "https://a.example", "addedAt": 1},
                    "https://b.example": {"url": "https://b.example", "addedAt": 2},
                    "https://c.example": {"url": "https://c.example", "addedAt": 3}
                }
            }))
            .unwrap(),
        );
        state.db.save_provider("codex", &edited).unwrap();
        assert!(matches!(
            activate_reviewed_provider(&team, &state, "codex", Some(&imported.decision_token)),
            Err(TeamProviderError::PreviewStale)
        ));
        assert!(state.db.get_current_provider("codex").unwrap().is_none());
        let latest = preview_provider(&team, &state, "codex", Some("approved-model")).unwrap();
        for _ in 0..16 {
            assert_eq!(
                preview_provider(&team, &state, "codex", Some("approved-model"))
                    .unwrap()
                    .decision_token,
                latest.decision_token,
                "reloading unchanged metadata must preserve the reviewed state"
            );
        }
        activate_reviewed_provider(&team, &state, "codex", Some(&latest.decision_token)).unwrap();
        assert_eq!(
            state.db.get_current_provider("codex").unwrap().as_deref(),
            Some(latest.provider_id.as_str())
        );
        std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
    }

    #[test]
    #[serial]
    fn review_recognizes_local_current_when_database_current_is_missing() {
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", root.path());
        crate::settings::reload_settings().unwrap();
        let credentials = Arc::new(MemoryCredentialStore::default());
        let team =
            TeamService::with_credentials(&root.path().join("team"), credentials.clone()).unwrap();
        let mut connection = connection();
        connection.id =
            super::super::connection_id(&connection.gateway_url, &connection.profile).unwrap();
        team.state.save_connection(&connection).unwrap();
        credentials
            .set(&connection.id, "nx_current_review_fixture")
            .unwrap();
        let state = AppState::new(Arc::new(Database::memory().unwrap()));
        let imported =
            apply_provider(&team, &state, "codex", Some("approved-model"), false).unwrap();
        crate::settings::set_current_provider(&AppType::Codex, Some(&imported.provider_id))
            .unwrap();
        credentials
            .set(&connection.id, "nx_rotated_review_fixture")
            .unwrap();
        let preview = preview_provider(&team, &state, "codex", Some("approved-model")).unwrap();
        assert!(
            preview.active,
            "local current state must be reflected in the preview"
        );
        assert!(matches!(
            apply_provider(&team, &state, "codex", Some("approved-model"), true),
            Err(TeamProviderError::ActiveProviderUpdate)
        ));
        std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
    }

    #[test]
    fn grok_merge_removes_only_the_recorded_team_model() {
        let current = r#"
[models]
default = "custom-c"

[model.old-team]
model = "old-team"
api_key = "old-key"
user_note = "keep-me"

[model.custom-c]
model = "custom-c"
api_key = "personal-key"
custom = true
"#;
        let target = r#"
[models]
default = "new-team"

[model.new-team]
model = "new-team"
base_url = "https://gateway.example/v1"
name = "NexusOps"
api_key = "new-key"
api_backend = "openai"
context_window = 131072
"#;
        let merged =
            merge_managed_toml(&AppType::GrokBuild, current, target, Some("old-team")).unwrap();
        let merged: toml::Value = toml::from_str(&merged).unwrap();
        assert_eq!(
            merged["model"]["old-team"]["user_note"].as_str(),
            Some("keep-me")
        );
        assert!(merged["model"]["old-team"].get("api_key").is_none());
        assert_eq!(merged["model"]["custom-c"]["custom"].as_bool(), Some(true));
        assert_eq!(
            merged["model"]["custom-c"]["api_key"].as_str(),
            Some("personal-key")
        );
        assert_eq!(merged["models"]["default"].as_str(), Some("new-team"));
        assert_eq!(
            merged["model"]["new-team"]["api_key"].as_str(),
            Some("new-key")
        );

        let legacy = merge_managed_toml(&AppType::GrokBuild, current, target, None).unwrap();
        let legacy: toml::Value = toml::from_str(&legacy).unwrap();
        assert!(legacy["model"].get("custom-c").is_some());
        assert!(legacy["model"].get("old-team").is_some());
    }

    #[test]
    #[serial]
    fn ownership_link_without_provider_is_recreated_instead_of_colliding() {
        let temp = tempfile::tempdir().unwrap();
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", temp.path());
        crate::settings::reload_settings().unwrap();
        let credentials = Arc::new(MemoryCredentialStore::default());
        let team =
            TeamService::with_credentials(&temp.path().join("team"), credentials.clone()).unwrap();
        let mut connection = connection();
        connection.id = super::super::connection_id(&connection.gateway_url, &connection.profile)
            .expect("derive connection id");
        team.state.save_connection(&connection).unwrap();
        credentials
            .set(&connection.id, "nx_synthetic_recreate")
            .unwrap();
        let provider_id = stable_provider_id(&connection.id, &AppType::Codex);
        team.state
            .save_provider_link(&ManagedProviderLink {
                connection_id: connection.id.clone(),
                app: "codex".into(),
                provider_id: provider_id.clone(),
                managed_model: Some("approved-model".into()),
            })
            .unwrap();
        let state = AppState::new(Arc::new(Database::memory().unwrap()));

        let preview = preview_provider(&team, &state, "codex", Some("approved-model")).unwrap();
        assert_eq!(preview.change, ProviderChange::Recreate);
        apply_provider(&team, &state, "codex", Some("approved-model"), false).unwrap();
        assert!(state
            .db
            .get_provider_by_id(&provider_id, "codex")
            .unwrap()
            .is_some());
        std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
    }

    #[test]
    #[serial]
    fn scrub_preflights_every_active_provider_before_mutating_any() {
        let temp = tempfile::tempdir().unwrap();
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", temp.path());
        crate::settings::reload_settings().unwrap();
        let credentials = Arc::new(MemoryCredentialStore::default());
        let team =
            TeamService::with_credentials(&temp.path().join("team"), credentials.clone()).unwrap();
        let mut connection = connection();
        connection.profile.key.allow_messages_dispatch = true;
        connection.id = super::super::connection_id(&connection.gateway_url, &connection.profile)
            .expect("derive connection id");
        team.state.save_connection(&connection).unwrap();
        credentials
            .set(&connection.id, "nx_synthetic_scrub")
            .unwrap();
        let state = AppState::new(Arc::new(Database::memory().unwrap()));

        let mut claude = build_team_provider(
            &connection,
            &AppType::Claude,
            Some("approved-model"),
            "nx_synthetic_scrub",
        )
        .unwrap();
        claude.id = stable_provider_id(&connection.id, &AppType::Claude);
        state.db.save_provider("claude", &claude).unwrap();
        state.db.set_current_provider("claude", &claude.id).unwrap();
        let mut personal = claude.clone();
        personal.id = "personal-claude".into();
        personal.name = "Personal".into();
        state.db.save_provider("claude", &personal).unwrap();
        team.state
            .save_provider_link(&ManagedProviderLink {
                connection_id: connection.id.clone(),
                app: "claude".into(),
                provider_id: claude.id.clone(),
                managed_model: Some("approved-model".into()),
            })
            .unwrap();

        let mut codex = build_team_provider(
            &connection,
            &AppType::Codex,
            Some("approved-model"),
            "nx_synthetic_scrub",
        )
        .unwrap();
        codex.id = stable_provider_id(&connection.id, &AppType::Codex);
        state.db.save_provider("codex", &codex).unwrap();
        state.db.set_current_provider("codex", &codex.id).unwrap();
        team.state
            .save_provider_link(&ManagedProviderLink {
                connection_id: connection.id.clone(),
                app: "codex".into(),
                provider_id: codex.id.clone(),
                managed_model: Some("approved-model".into()),
            })
            .unwrap();

        assert!(matches!(
            scrub_provider_credentials(&team, &state),
            Err(TeamProviderError::NoFallbackProvider)
        ));
        assert!(state
            .db
            .get_provider_by_id(&claude.id, "claude")
            .unwrap()
            .is_some());
        assert!(state
            .db
            .get_provider_by_id(&codex.id, "codex")
            .unwrap()
            .is_some());

        let mut personal_codex = codex.clone();
        personal_codex.id = "personal-codex".into();
        personal_codex.name = "Personal Codex".into();
        state.db.save_provider("codex", &personal_codex).unwrap();
        // Reproduce local-settings/SQLite divergence. Preflight must treat
        // either source as active before mutating the first app.
        state
            .db
            .set_current_provider("claude", &personal.id)
            .unwrap();
        crate::settings::set_current_provider(&AppType::Claude, Some(&claude.id)).unwrap();
        crate::settings::set_current_provider(&AppType::Codex, Some(&codex.id)).unwrap();
        assert_eq!(scrub_provider_credentials(&team, &state).unwrap(), 2);
        assert!(state
            .db
            .get_provider_by_id(&claude.id, "claude")
            .unwrap()
            .is_none());
        assert!(state
            .db
            .get_provider_by_id(&codex.id, "codex")
            .unwrap()
            .is_none());
        std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
    }

    #[test]
    #[serial]
    fn repeat_import_rotation_and_activation_preserve_personal_provider() {
        let temp = tempfile::tempdir().unwrap();
        std::env::set_var("NEXUSOPS_CLIENT_TEST_HOME", temp.path());
        crate::settings::reload_settings().unwrap();
        let credentials = Arc::new(MemoryCredentialStore::default());
        let team =
            TeamService::with_credentials(&temp.path().join("team"), credentials.clone()).unwrap();
        let mut connection = connection();
        connection.id = super::super::connection_id(&connection.gateway_url, &connection.profile)
            .expect("derive connection id");
        team.state.save_connection(&connection).unwrap();
        credentials
            .set(&connection.id, "nx_synthetic_first")
            .unwrap();
        let state = AppState::new(Arc::new(Database::memory().unwrap()));

        let mut personal = build_team_provider(
            &connection,
            &AppType::Codex,
            Some("approved-model"),
            "nx_personal_fixture",
        )
        .unwrap();
        personal.id = "personal-provider".into();
        personal.name = "Personal provider".into();
        state
            .db
            .save_provider(AppType::Codex.as_str(), &personal)
            .unwrap();
        state
            .db
            .set_current_provider(AppType::Codex.as_str(), &personal.id)
            .unwrap();
        crate::settings::set_current_provider(&AppType::Codex, Some(&personal.id)).unwrap();

        let initial = preview_provider(&team, &state, "codex", Some("approved-model")).unwrap();
        assert_eq!(initial.change, ProviderChange::Create);
        let imported =
            apply_provider(&team, &state, "codex", Some("approved-model"), false).unwrap();
        assert_eq!(imported.change, ProviderChange::Unchanged);
        assert_eq!(
            state
                .db
                .get_current_provider(AppType::Codex.as_str())
                .unwrap()
                .as_deref(),
            Some("personal-provider")
        );
        apply_provider(&team, &state, "codex", Some("approved-model"), false).unwrap();
        assert_eq!(team.state.provider_links(&connection.id).unwrap().len(), 1);

        let link = team.state.provider_links(&connection.id).unwrap().remove(0);
        let mut customized = state
            .db
            .get_provider_by_id(&link.provider_id, AppType::Codex.as_str())
            .unwrap()
            .unwrap();
        customized.settings_config["auth"]["USER_DEFINED"] =
            serde_json::Value::String("keep-me".into());
        let config = customized.settings_config["config"].as_str().unwrap();
        customized.settings_config["config"] =
            serde_json::Value::String(format!("{config}\nuser_defined = \"keep-me\"\n"));
        state
            .db
            .save_provider(AppType::Codex.as_str(), &customized)
            .unwrap();

        credentials
            .set(&connection.id, "nx_synthetic_rotated")
            .unwrap();
        let rotation = preview_provider(&team, &state, "codex", Some("approved-model")).unwrap();
        assert_eq!(rotation.change, ProviderChange::UpdateRequiresConfirmation);
        assert!(matches!(
            apply_provider(&team, &state, "codex", Some("approved-model"), false),
            Err(TeamProviderError::ConfirmationRequired)
        ));
        let updated = apply_provider(&team, &state, "codex", Some("approved-model"), true).unwrap();
        assert_eq!(updated.change, ProviderChange::Unchanged);
        let stored = state
            .db
            .get_provider_by_id(&updated.provider_id, AppType::Codex.as_str())
            .unwrap()
            .unwrap();
        let stored_json = serde_json::to_string(&stored).unwrap();
        assert!(stored_json.contains("nx_synthetic_rotated"));
        assert!(!stored_json.contains("nx_synthetic_first"));
        assert!(stored_json.contains("USER_DEFINED"));
        assert!(stored_json.contains("keep-me"));

        activate_provider(&team, &state, "codex").unwrap();
        assert_eq!(
            state
                .db
                .get_current_provider(AppType::Codex.as_str())
                .unwrap()
                .as_deref(),
            Some(updated.provider_id.as_str())
        );
        assert!(state
            .db
            .get_provider_by_id("personal-provider", AppType::Codex.as_str())
            .unwrap()
            .is_some());
        credentials
            .set(&connection.id, "nx_synthetic_after_activation")
            .unwrap();
        assert!(matches!(
            apply_provider(&team, &state, "codex", Some("approved-model"), true),
            Err(TeamProviderError::ActiveProviderUpdate)
        ));
        let still_stored = state
            .db
            .get_provider_by_id(&updated.provider_id, AppType::Codex.as_str())
            .unwrap()
            .unwrap();
        let still_stored = serde_json::to_string(&still_stored).unwrap();
        assert!(still_stored.contains("nx_synthetic_rotated"));
        assert!(!still_stored.contains("nx_synthetic_after_activation"));
        std::env::remove_var("NEXUSOPS_CLIENT_TEST_HOME");
    }
}
