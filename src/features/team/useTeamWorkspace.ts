import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  activateTeamProvider,
  applyTeamProvider,
  cancelTeamOperation,
  connectTeam,
  disconnectTeam,
  getTeamLocalHistory,
  getTeamStatus,
  previewTeamProvider,
  previewTeamSync,
  restoreTeamBackup,
  syncTeam,
  type ProviderPreview,
  type LocalAssetHistory,
  type SyncBatchResult,
  type SyncPlan,
  type TeamCommandError,
  type TeamConnection,
  type TeamManifest,
} from "./api";

export const PROVIDER_APPS = [
  "claude",
  "codex",
  "gemini",
  "grokbuild",
  "opencode",
];
export const NO_MODEL = "__gateway_default__";

type BusyAction =
  | "profile"
  | "connect"
  | "refresh"
  | "disconnect"
  | "provider"
  | "restore"
  | "sync"
  | null;

function commandError(error: unknown): TeamCommandError {
  if (error && typeof error === "object") {
    const value = error as Partial<TeamCommandError>;
    if (typeof value.code === "string" && typeof value.message === "string") {
      return { code: value.code, message: value.message };
    }
  }
  return {
    code: "unknown",
    message: error instanceof Error ? error.message : String(error),
  };
}

function readTextFile(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () =>
      typeof reader.result === "string"
        ? resolve(reader.result)
        : reject(new Error("Team Profile is not text"));
    reader.onerror = () =>
      reject(reader.error ?? new Error("Could not read Team Profile"));
    reader.readAsText(file, "utf-8");
  });
}

function supportedProviderApps(connection: TeamConnection | null): string[] {
  if (!connection) return [];
  const platform = connection.profile.key.platform.toLowerCase();
  return PROVIDER_APPS.filter((app) => {
    if (app === "claude") {
      return (
        ["anthropic", "antigravity", "grok"].includes(platform) ||
        (platform === "openai" &&
          connection.profile.key.allow_messages_dispatch)
      );
    }
    if (app === "codex") return platform === "openai" || platform === "grok";
    if (app === "gemini")
      return platform === "gemini" || platform === "antigravity";
    if (app === "grokbuild") return platform === "grok";
    return false;
  });
}

export function useTeamWorkspace(initialApp = "codex") {
  const { t } = useTranslation();
  const [connection, setConnection] = useState<TeamConnection | null>(null);
  const [manifest, setManifest] = useState<TeamManifest | null>(null);
  const [gateway, setGateway] = useState("");
  const [memberKey, setMemberKey] = useState("");
  const [importedProfileName, setImportedProfileName] = useState<string | null>(
    null,
  );
  const initialTarget = PROVIDER_APPS.includes(initialApp)
    ? initialApp
    : "codex";
  const [providerApp, setProviderApp] = useState(initialTarget);
  const [assetApp, setAssetApp] = useState(initialTarget);
  const [model, setModel] = useState(NO_MODEL);
  const [providerPreview, setProviderPreview] =
    useState<ProviderPreview | null>(null);
  const [syncPlan, setSyncPlan] = useState<SyncPlan | null>(null);
  const [syncResult, setSyncResult] = useState<SyncBatchResult | null>(null);
  const [localHistory, setLocalHistory] = useState<LocalAssetHistory[]>([]);
  const [localHistoryApp, setLocalHistoryApp] = useState("codex");
  const [lastSyncAt, setLastSyncAt] = useState<string | null>(null);
  const [overwriteAssets, setOverwriteAssets] = useState<Set<number>>(
    new Set(),
  );
  const [showDisconnect, setShowDisconnect] = useState(false);
  const [removeProviderCredentials, setRemoveProviderCredentials] =
    useState(true);
  const [pendingRestoreAsset, setPendingRestoreAsset] = useState<{
    id: number;
    name: string;
    app: string;
  } | null>(null);
  const [busy, setBusy] = useState<BusyAction>(null);
  const [error, setError] = useState<TeamCommandError | null>(null);
  const requestRevision = useRef(0);

  const selectedModel = model === NO_MODEL ? null : model;
  const models = connection?.profile.models ?? [];
  const providerApps = useMemo(
    () => supportedProviderApps(connection),
    [connection],
  );

  useEffect(() => {
    if (!connection || syncPlan) return;
    let active = true;
    setLocalHistory([]);
    setLocalHistoryApp(assetApp);
    void getTeamLocalHistory(assetApp)
      .then((history) => {
        if (active) setLocalHistory(history);
      })
      .catch(() => {
        if (active) setLocalHistory([]);
      });
    return () => {
      active = false;
    };
  }, [assetApp, connection, syncPlan]);

  const statusLabel = useMemo(() => {
    if (!connection) return "";
    return t(`team.status.${connection.status}`);
  }, [connection, t]);
  const lastSuccessfulSync = useMemo(() => {
    return lastSyncAt
      ? new Date(lastSyncAt).toLocaleString()
      : t("team.connection.neverSynced");
  }, [lastSyncAt, t]);

  useEffect(() => {
    if (providerApps.length > 0 && !providerApps.includes(providerApp)) {
      setProviderApp(providerApps[0]);
      setProviderPreview(null);
      setSyncPlan(null);
    }
  }, [providerApp, providerApps]);

  useEffect(() => {
    if (models.length === 0) {
      setModel(NO_MODEL);
      setProviderPreview(null);
      return;
    }
    if (!models.some((candidate) => candidate.id === model)) {
      setModel(models[0].id);
      setProviderPreview(null);
    }
  }, [model, models]);

  useEffect(() => {
    let active = true;
    const revision = ++requestRevision.current;
    void getTeamStatus()
      .then((status) => {
        if (!active || revision !== requestRevision.current) return;
        setConnection(status);
        if (status) {
          setGateway(status.gateway_url);
          return previewTeamSync(initialTarget).then((plan) => {
            if (!active || revision !== requestRevision.current) return;
            setConnection(plan.connection);
            setManifest({
              schema_version: 1,
              assets: plan.items.map((item) => item.asset),
              conflicts: plan.conflicts,
            });
            setSyncPlan(plan);
            setLastSyncAt(plan.last_successful_sync);
          });
        }
      })
      .catch((reason) => {
        if (!active || revision !== requestRevision.current) return;
        setError(commandError(reason));
        void getTeamStatus()
          .then((status) => {
            if (active && revision === requestRevision.current)
              setConnection(status);
          })
          .catch(() => {
            // Keep the original actionable error when local status recovery also fails.
          });
      });
    return () => {
      active = false;
      requestRevision.current += 1;
      void cancelTeamOperation();
    };
  }, []);

  const runRefresh = async () => {
    const revision = ++requestRevision.current;
    const requestedApp = assetApp;
    setBusy("refresh");
    setError(null);
    try {
      const plan = await previewTeamSync(requestedApp);
      if (revision !== requestRevision.current) return;
      setConnection(plan.connection);
      setManifest({
        schema_version: 1,
        assets: plan.items.map((item) => item.asset),
        conflicts: plan.conflicts,
      });
      setProviderPreview(null);
      setSyncPlan(plan);
      setSyncResult(null);
      setOverwriteAssets(new Set());
      setLastSyncAt(plan.last_successful_sync);
      setPendingRestoreAsset(null);
    } catch (reason) {
      setError(commandError(reason));
      try {
        setConnection(await getTeamStatus());
      } catch {
        // Preserve the actionable request error when local status cannot reload.
      }
    } finally {
      setBusy(null);
    }
  };

  const handleConnect = async (event: React.FormEvent) => {
    event.preventDefault();
    if (busy !== null || !gateway.trim() || !memberKey.trim()) return;
    const revision = ++requestRevision.current;
    setBusy("connect");
    setError(null);
    try {
      const result = await connectTeam(gateway.trim(), memberKey.trim());
      if (revision !== requestRevision.current) return;
      setConnection(result);
      setGateway(result.gateway_url);
      const plan = await previewTeamSync(assetApp);
      if (revision !== requestRevision.current) return;
      setConnection(plan.connection);
      setManifest({
        schema_version: 1,
        assets: plan.items.map((item) => item.asset),
        conflicts: plan.conflicts,
      });
      setSyncPlan(plan);
      setLastSyncAt(plan.last_successful_sync);
      setImportedProfileName(null);
    } catch (reason) {
      setError(commandError(reason));
    } finally {
      setMemberKey("");
      setBusy(null);
    }
  };

  const handleProfileFile = async (
    event: React.ChangeEvent<HTMLInputElement>,
  ) => {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = "";
    if (!file) return;
    const revision = ++requestRevision.current;
    setBusy("profile");
    setError(null);
    setImportedProfileName(null);
    setGateway("");
    setMemberKey("");
    try {
      if (file.size > 1_048_576) throw new Error("Team Profile exceeds 1 MiB");
      const text = await readTextFile(file);
      if (revision !== requestRevision.current) return;
      const value = JSON.parse(text) as Record<string, unknown> | null;
      if (
        !value ||
        value.schema_version !== 1 ||
        typeof value.gateway_url !== "string" ||
        typeof value.member_key !== "string" ||
        !value.gateway_url.trim() ||
        !value.member_key.trim()
      ) {
        throw new Error("Unsupported Team Profile");
      }
      setGateway(value.gateway_url.trim());
      setMemberKey(value.member_key.trim());
      setImportedProfileName(
        typeof value.name === "string" && value.name.trim()
          ? value.name.trim()
          : t("team.connect.importedProfile"),
      );
    } catch {
      if (revision !== requestRevision.current) return;
      // JSON parse errors may contain fragments of the credential-bearing file.
      setError({
        code: "invalid_profile_file",
        message: t("team.errors.invalid_profile_file"),
      });
    } finally {
      if (revision === requestRevision.current) setBusy(null);
    }
  };

  const handleDisconnect = async () => {
    requestRevision.current += 1;
    setBusy("disconnect");
    setError(null);
    try {
      await disconnectTeam(removeProviderCredentials);
      setConnection(null);
      setManifest(null);
      setProviderPreview(null);
      setSyncPlan(null);
      setSyncResult(null);
      setLocalHistory([]);
      setOverwriteAssets(new Set());
      setLastSyncAt(null);
      setShowDisconnect(false);
      setModel(NO_MODEL);
    } catch (reason) {
      setError(commandError(reason));
      try {
        setConnection(await getTeamStatus());
      } catch {
        // Keep the sync error visible when local status cannot reload.
      }
    } finally {
      setBusy(null);
    }
  };

  const handleProviderFailure = async (reason: unknown, revision: number) => {
    if (revision !== requestRevision.current) return;
    setError(commandError(reason));
    setProviderPreview(null);
    try {
      const status = await getTeamStatus();
      if (revision === requestRevision.current) setConnection(status);
    } catch {
      // Keep the operation error if the stored connection cannot be read.
    }
  };

  const handleProviderPreview = async () => {
    const revision = ++requestRevision.current;
    const requestedApp = providerApp;
    const requestedModel = selectedModel;
    setBusy("provider");
    setError(null);
    try {
      const preview = await previewTeamProvider(requestedApp, requestedModel);
      if (revision === requestRevision.current) setProviderPreview(preview);
    } catch (reason) {
      await handleProviderFailure(reason, revision);
    } finally {
      if (revision === requestRevision.current) setBusy(null);
    }
  };

  const handleProviderApply = async () => {
    if (!providerPreview) return;
    const revision = ++requestRevision.current;
    const requestedApp = providerPreview.app;
    const requestedModel = providerPreview.selected_model;
    setBusy("provider");
    setError(null);
    try {
      const preview = await applyTeamProvider(
        requestedApp,
        requestedModel,
        providerPreview.change === "update_requires_confirmation",
        providerPreview.decision_token,
      );
      if (revision === requestRevision.current) setProviderPreview(preview);
    } catch (reason) {
      await handleProviderFailure(reason, revision);
    } finally {
      if (revision === requestRevision.current) setBusy(null);
    }
  };

  const handleProviderActivate = async () => {
    if (!providerPreview) return;
    const revision = ++requestRevision.current;
    const requestedApp = providerPreview.app;
    const requestedModel = providerPreview.selected_model;
    setBusy("provider");
    setError(null);
    try {
      await activateTeamProvider(requestedApp, providerPreview.decision_token);
      const preview = await previewTeamProvider(requestedApp, requestedModel);
      if (revision === requestRevision.current) setProviderPreview(preview);
    } catch (reason) {
      await handleProviderFailure(reason, revision);
    } finally {
      if (revision === requestRevision.current) setBusy(null);
    }
  };

  const handleSyncPreview = async () => {
    const revision = ++requestRevision.current;
    const requestedApp = assetApp;
    setBusy("sync");
    setError(null);
    try {
      const plan = await previewTeamSync(requestedApp);
      if (revision !== requestRevision.current) return;
      setConnection(plan.connection);
      setManifest({
        schema_version: 1,
        assets: plan.items.map((item) => item.asset),
        conflicts: plan.conflicts,
      });
      setSyncPlan(plan);
      setSyncResult(null);
      setOverwriteAssets(new Set());
      setLastSyncAt(plan.last_successful_sync);
      setPendingRestoreAsset(null);
    } catch (reason) {
      setError(commandError(reason));
      try {
        setConnection(await getTeamStatus());
      } catch {
        // Keep the sync error visible when local status cannot reload.
      }
    } finally {
      setBusy(null);
    }
  };

  const handleSync = async () => {
    if (!syncPlan) return;
    const revision = ++requestRevision.current;
    setBusy("sync");
    setError(null);
    try {
      const decisions = [...overwriteAssets].flatMap((assetId) => {
        const item = syncPlan.items.find(
          (candidate) => candidate.asset.asset_id === assetId,
        );
        return item
          ? [{ asset_id: assetId, decision_token: item.decision_token }]
          : [];
      });
      const result = await syncTeam(assetApp, decisions);
      if (revision !== requestRevision.current) return;
      setConnection(result.connection);
      setSyncResult(result);
      setLastSyncAt(result.last_successful_sync);
      setSyncPlan(null);
      setOverwriteAssets(new Set());
    } catch (reason) {
      setError(commandError(reason));
      try {
        setConnection(await getTeamStatus());
      } catch {
        // Keep the sync error visible when local status cannot reload.
      }
    } finally {
      setBusy(null);
    }
  };

  const handleRestore = async (assetId: number, restoreApp: string) => {
    const revision = ++requestRevision.current;
    setBusy("restore");
    setError(null);
    let recoveryWarning: TeamCommandError | null = null;
    try {
      const restored = await restoreTeamBackup(restoreApp, assetId);
      setPendingRestoreAsset(null);
      if (restored.upstream_pending) {
        recoveryWarning = {
          code: "upstream_pending",
          message:
            restored.error_message ??
            "The local backup was restored, but the tool catalog still needs recovery.",
        };
      }
    } catch (reason) {
      setError(commandError(reason));
      setBusy(null);
      return;
    }

    try {
      setLocalHistory(await getTeamLocalHistory(restoreApp));
      setLocalHistoryApp(restoreApp);
    } catch {
      // The restore is already committed; keep the prior local list if it cannot reload.
    }
    try {
      const plan = await previewTeamSync(restoreApp);
      if (revision !== requestRevision.current) return;
      setConnection(plan.connection);
      setManifest({
        schema_version: 1,
        assets: plan.items.map((item) => item.asset),
        conflicts: plan.conflicts,
      });
      setSyncPlan(plan);
      setSyncResult(null);
      setOverwriteAssets(new Set());
      setLastSyncAt(plan.last_successful_sync);
      setError(recoveryWarning);
    } catch {
      setError({
        code: "restore_refresh_failed",
        message: t("team.errorActions.restore_refresh_failed"),
      });
    } finally {
      setBusy(null);
    }
  };

  const setOverwrite = (assetId: number, enabled: boolean) => {
    setOverwriteAssets((current) => {
      const next = new Set(current);
      if (enabled) next.add(assetId);
      else next.delete(assetId);
      return next;
    });
  };

  return {
    connection,
    error,
    busy,
    handleConnect,
    handleProfileFile,
    importedProfileName,
    gateway,
    setGateway,
    memberKey,
    setMemberKey,
    statusLabel,
    runRefresh,
    setShowDisconnect,
    showDisconnect,
    removeProviderCredentials,
    setRemoveProviderCredentials,
    handleDisconnect,
    models,
    lastSuccessfulSync,
    providerApp,
    requestRevision,
    setProviderApp,
    setProviderPreview,
    providerApps,
    model,
    setModel,
    providerPreview,
    handleProviderPreview,
    selectedModel,
    handleProviderApply,
    handleProviderActivate,
    assetApp,
    setAssetApp,
    setSyncPlan,
    setSyncResult,
    setOverwriteAssets,
    setLastSyncAt,
    setPendingRestoreAsset,
    setLocalHistory,
    setLocalHistoryApp,
    manifest,
    handleSyncPreview,
    syncPlan,
    handleSync,
    pendingRestoreAsset,
    handleRestore,
    syncResult,
    overwriteAssets,
    setOverwrite,
    localHistoryApp,
    localHistory,
  };
}
export type TeamWorkspaceModel = ReturnType<typeof useTeamWorkspace>;
