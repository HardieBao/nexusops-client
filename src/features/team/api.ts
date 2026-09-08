import { invoke } from "@tauri-apps/api/core";

export type ConnectionStatus =
  | "connected"
  | "authentication_required"
  | "access_denied"
  | "unavailable";

export interface ProfileModel {
  id: string;
  name: string;
  platform: string;
}

export interface TeamProfile {
  schema_version: number;
  organization_id: string;
  workspace_id: string;
  name: string;
  gateway_url: string;
  base_url: string;
  key: {
    id: number;
    name: string;
    prefix: string;
    last_four: string;
    platform: string;
    allow_messages_dispatch: boolean;
  };
  models: ProfileModel[];
  asset_manifest_url: string;
  subscriptions: string[];
  limits: {
    max_assets: number;
    max_revisions_per_asset: number;
    max_text_bytes: number;
    max_archive_bytes: number;
    max_unpacked_bytes: number;
    max_files: number;
    max_path_depth: number;
    max_path_bytes: number;
  };
}

export interface TeamConnection {
  id: string;
  gateway_url: string;
  profile: TeamProfile;
  status: ConnectionStatus;
  last_checked_at: string;
  last_error: string | null;
}

export type AssetKind = "prompt" | "skill" | "rule" | "workflow" | "agent";

export interface ManifestItem {
  asset_id: number;
  kind: AssetKind;
  slug: string;
  name: string;
  revision: number;
  content_hash: string;
  archive_sha256: string | null;
  download_url: string;
  content_type: string;
  byte_size: number;
  files: Array<{
    path: string;
    sha256: string;
    size: number;
    executable: boolean;
  }>;
}

export interface TeamManifest {
  schema_version: number;
  assets: ManifestItem[];
  conflicts: Array<{ asset_id: number; slug: string; reason: string }>;
}

export interface TeamRefresh {
  connection: TeamConnection;
  manifest: TeamManifest;
}

export type ProviderChange =
  | "create"
  | "unchanged"
  | "recreate"
  | "update_requires_confirmation";

export interface ProviderPreview {
  app: string;
  provider_id: string;
  name: string;
  base_url: string;
  authorized_models: string[];
  selected_model: string | null;
  active: boolean;
  change: ProviderChange;
  changed_fields: string[];
  decision_token: string;
}

export interface TeamCommandError {
  code: string;
  message: string;
}

export type DriftStatus =
  | "not_installed"
  | "same"
  | "remote_update"
  | "local_modified"
  | "both_modified";

export type SyncSupport = "tool_skill" | "inactive_prompt" | "managed_download";

export interface SyncPlanItem {
  asset: ManifestItem;
  drift: DriftStatus;
  support: SyncSupport;
  install_path: string;
  previous_revision: number | null;
  subscribed: boolean;
  has_backup: boolean;
  last_synced_at: string | null;
  decision_token: string;
  disk_fingerprint: string | null;
  upstream_fingerprint: string | null;
  inspection_error_code: string | null;
  inspection_error_message: string | null;
}

export interface SyncPlan {
  connection: TeamConnection;
  conflicts: TeamManifest["conflicts"];
  items: SyncPlanItem[];
  withdrawn: LocalAssetHistory[];
  last_successful_sync: string | null;
}

export interface LocalAssetHistory {
  asset_id: number;
  name: string;
  kind: AssetKind | null;
  revision: number;
  last_synced_at: string | null;
  has_backup: boolean;
  local_file_present: boolean;
  recovery_error: string | null;
}

export type SyncOutcome =
  | "installed"
  | "downloaded"
  | "imported_inactive"
  | "unchanged"
  | "skipped"
  | "conflict"
  | "failed";

export interface SyncItemResult {
  asset_id: number;
  revision: number;
  outcome: SyncOutcome;
  drift: DriftStatus;
  install_path: string;
  backup_path: string | null;
  error_code: string | null;
  error_message: string | null;
}

export interface SyncBatchResult {
  connection: TeamConnection;
  conflicts: TeamManifest["conflicts"];
  items: SyncItemResult[];
  last_successful_sync: string | null;
}

export interface LocalRestoreResult {
  asset_id: number;
  restored: boolean;
  has_undo_backup: boolean;
  upstream_pending: boolean;
  error_message: string | null;
}

export interface SyncOverwriteDecision {
  asset_id: number;
  decision_token: string;
}

export function getTeamStatus() {
  return invoke<TeamConnection | null>("team_status");
}

export function connectTeam(gateway: string, key: string) {
  return invoke<TeamConnection>("team_connect", { gateway, key });
}

export function refreshTeam() {
  return invoke<TeamRefresh>("team_refresh");
}

export function cancelTeamOperation() {
  return invoke<void>("team_cancel");
}

export function disconnectTeam(removeProviderCredentials: boolean) {
  return invoke<void>("team_disconnect", { removeProviderCredentials });
}

export function previewTeamProvider(app: string, model: string | null) {
  return invoke<ProviderPreview>("team_preview_provider", { app, model });
}

export function applyTeamProvider(
  app: string,
  model: string | null,
  confirmUpdate: boolean,
  decisionToken: string,
) {
  return invoke<ProviderPreview>("team_apply_provider", {
    app,
    model,
    confirmUpdate,
    decisionToken,
  });
}

export function activateTeamProvider(app: string, decisionToken: string) {
  return invoke<void>("team_activate_provider", { app, decisionToken });
}

export function previewTeamSync(app: string) {
  return invoke<SyncPlan>("team_preview_sync", { app });
}

export function syncTeam(app: string, overwriteLocal: SyncOverwriteDecision[]) {
  return invoke<SyncBatchResult>("team_sync", { app, overwriteLocal });
}

export function restoreTeamBackup(app: string, assetId: number) {
  return invoke<LocalRestoreResult>("team_restore_backup", { app, assetId });
}

export function getTeamLocalHistory(app: string) {
  return invoke<LocalAssetHistory[]>("team_local_history", { app });
}
