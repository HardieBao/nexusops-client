import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  open as openNative,
  save as saveNative,
} from "@tauri-apps/plugin-dialog";
import { dirname, basename, downloadDir, join } from "@tauri-apps/api/path";

import {
  activateTeamProvider,
  applyTeamProvider,
  cancelTeamOperation,
  waitForTeamIdle,
  connectTeam,
  disconnectTeam,
  getTeamLocalHistory,
  getTeamStatus,
  previewTeamProvider,
  previewTeamSync as previewLegacyTeamSync,
  previewTeamAI,
  applyTeamAI,
  retryTeamAIAcknowledgements,
  getTeamAIAcknowledgementStatus,
  exportTeamAICandidate,
  type TeamAICandidateExport,
  type TeamAIPreview,
  type TeamAIAcknowledgements,
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
  | "cancelling"
  | "reporting"
  | "export"
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

function readTextFile(file: File, signal: AbortSignal): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    const abort = () => {
      reader.abort();
      reject(new Error("Profile read cancelled"));
    };
    if (signal.aborted) {
      reject(new Error("Profile read cancelled"));
      return;
    }
    signal.addEventListener("abort", abort, { once: true });
    reader.onloadend = () => signal.removeEventListener("abort", abort);
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
  const [operationBusy, setBusy] = useState<BusyAction>(null);
  const [reportingBusy, setReportingBusyState] = useState(false);
  const busy = operationBusy ?? (reportingBusy ? "reporting" : null);
  const [error, setError] = useState<TeamCommandError | null>(null);
  const requestRevision = useRef(0);
  const [candidateExport, setCandidateExport] =
    useState<TeamAICandidateExport | null>(null);
  const signedPreviews = useRef(new WeakMap<SyncPlan, TeamAIPreview>());
  const [ackState, setAckState] = useState<{
    app: string;
    connectionId: string;
    summary: TeamAIAcknowledgements;
  } | null>(null);
  const acknowledgements =
    ackState?.app === assetApp && ackState.connectionId === connection?.id
      ? ackState.summary
      : null;
  const teamaiSync = syncPlan !== null && signedPreviews.current.has(syncPlan);
  const legacySync =
    syncPlan !== null &&
    !teamaiSync &&
    (assetApp === "codex" || assetApp === "claude");

  async function previewTeamSync(app: string): Promise<SyncPlan> {
    if (app !== "codex" && app !== "claude") return previewLegacyTeamSync(app);
    const owner = requestRevision.current;
    try {
      const preview = await previewTeamAI(app);
      if (owner !== requestRevision.current)
        throw { code: "cancelled", message: "The operation was superseded" };
      const plan = {
        ...preview.plan,
        items: [...preview.plan.items, ...preview.legacy_items].sort(
          (a, b) => a.asset.asset_id - b.asset.asset_id,
        ),
      };
      signedPreviews.current.set(plan, preview);
      return plan;
    } catch (reason) {
      if (
        owner !== requestRevision.current ||
        commandError(reason).code !== "upgrade_required"
      )
        throw reason;
      return previewLegacyTeamSync(app);
    }
  }
  const pendingOperations = useRef(new Set<Promise<void>>());
  const fileReads = useRef(new Set<AbortController>());
  const cancelling = useRef(false);
  const [operationNotice, setOperationNotice] = useState<
    "stopped" | "cancelFailed" | "recoveryFailed" | null
  >(null);
  const setReportingBusy = useCallback((value: boolean) => {
    setReportingBusyState(value);
    if (value) setOperationNotice(null);
  }, []);

  const selectedModel = model === NO_MODEL ? null : model;
  const models = connection?.profile.models ?? [];
  const providerApps = useMemo(
    () => supportedProviderApps(connection),
    [connection],
  );

  useEffect(() => {
    if (!connection || syncPlan) return;
    let active = true;
    const revision = requestRevision.current;
    setLocalHistory([]);
    setLocalHistoryApp(assetApp);
    void getTeamLocalHistory(assetApp)
      .then((history) => {
        if (active && revision === requestRevision.current)
          setLocalHistory(history);
      })
      .catch(() => {
        if (active && revision === requestRevision.current) setLocalHistory([]);
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
      for (const read of fileReads.current) read.abort();
      void cancelTeamOperation().catch(() => {});
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
      if (revision !== requestRevision.current) return;
      setError(commandError(reason));
      try {
        const status = await getTeamStatus();
        if (revision === requestRevision.current) setConnection(status);
      } catch {
        // Preserve the actionable request error when local status cannot reload.
      }
    } finally {
      if (revision === requestRevision.current) setBusy(null);
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
      if (revision !== requestRevision.current) return;
      setError(commandError(reason));
    } finally {
      if (revision === requestRevision.current) {
        setMemberKey("");
        setBusy(null);
      }
    }
  };

  const handleProfileFile = async (
    event: React.ChangeEvent<HTMLInputElement>,
  ) => {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = "";
    if (!file) return;
    const revision = ++requestRevision.current;
    for (const read of fileReads.current) read.abort();
    const read = new AbortController();
    fileReads.current.add(read);
    setBusy("profile");
    setError(null);
    setImportedProfileName(null);
    setGateway("");
    setMemberKey("");
    try {
      if (file.size > 1_048_576) throw new Error("Team Profile exceeds 1 MiB");
      const text = await readTextFile(file, read.signal);
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
      fileReads.current.delete(read);
      if (revision === requestRevision.current) setBusy(null);
    }
  };

  const handleDisconnect = async () => {
    const revision = ++requestRevision.current;
    setBusy("disconnect");
    setError(null);
    try {
      await disconnectTeam(removeProviderCredentials);
      if (revision !== requestRevision.current) return;
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
      setMemberKey("");
      setGateway("");
      setImportedProfileName(null);
    } catch (reason) {
      if (revision !== requestRevision.current) return;
      setError(commandError(reason));
      try {
        const status = await getTeamStatus();
        if (revision === requestRevision.current) setConnection(status);
      } catch {
        // Keep the sync error visible when local status cannot reload.
      }
    } finally {
      if (revision === requestRevision.current) setBusy(null);
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
      if (revision !== requestRevision.current) return;
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
      if (revision !== requestRevision.current) return;
      setError(commandError(reason));
      try {
        const status = await getTeamStatus();
        if (revision === requestRevision.current) setConnection(status);
      } catch {
        // Keep the sync error visible when local status cannot reload.
      }
    } finally {
      if (revision === requestRevision.current) setBusy(null);
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
      const review = signedPreviews.current.get(syncPlan);
      const applied = review
        ? await applyTeamAI(assetApp, review, decisions)
        : null;
      const result = applied
        ? applied.install
        : await syncTeam(assetApp, decisions);
      if (revision !== requestRevision.current) return;
      setAckState(
        applied
          ? {
              app: assetApp,
              connectionId: result.connection.id,
              summary: applied.acknowledgements,
            }
          : null,
      );
      setConnection(result.connection);
      setSyncResult(result);
      setLastSyncAt(result.last_successful_sync);
      setSyncPlan(null);
      setOverwriteAssets(new Set());
    } catch (reason) {
      if (revision !== requestRevision.current) return;
      setError(commandError(reason));
      try {
        const status = await getTeamStatus();
        if (revision === requestRevision.current) setConnection(status);
      } catch {
        // Keep the sync error visible when local status cannot reload.
      }
    } finally {
      if (revision === requestRevision.current) setBusy(null);
    }
  };

  const handleRetryAcknowledgements = async () => {
    if (!connection || (assetApp !== "codex" && assetApp !== "claude")) return;
    const revision = ++requestRevision.current;
    const requestedApp = assetApp;
    const connectionId = connection.id;
    setBusy("sync");
    setError(null);
    try {
      const summary = await retryTeamAIAcknowledgements(requestedApp);
      if (revision !== requestRevision.current) return;
      setAckState({ app: requestedApp, connectionId, summary });
      try {
        const status = await getTeamStatus();
        if (revision === requestRevision.current) setConnection(status);
      } catch {
        /* Confirmation succeeded; a local status read must not relabel it as failed. */
      }
    } catch (reason) {
      if (revision === requestRevision.current) setError(commandError(reason));
    } finally {
      if (revision === requestRevision.current) setBusy(null);
    }
  };

  const refreshAcknowledgements = useCallback(async () => {
    const connectionId = connection?.id;
    if (!connectionId || (assetApp !== "codex" && assetApp !== "claude"))
      return;
    const revision = requestRevision.current;
    try {
      const result = await getTeamAIAcknowledgementStatus(assetApp);
      if (
        revision !== requestRevision.current ||
        result.connection?.id !== connectionId
      )
        return;
      setConnection((current) =>
        current?.id === result.connection?.id &&
        current?.status === result.connection?.status &&
        current?.last_error === result.connection?.last_error &&
        current?.last_checked_at === result.connection?.last_checked_at
          ? current
          : result.connection,
      );
      setAckState((current) => {
        const same =
          current?.app === assetApp && current.connectionId === connectionId;
        if (
          same &&
          current.summary.waiting === result.status.pending &&
          current.summary.last_error === result.status.last_error
        )
          return current;
        if (!same && result.status.pending === 0 && !result.status.last_error)
          return null;
        return {
          app: assetApp,
          connectionId,
          summary: {
            waiting: result.status.pending,
            acknowledged: 0,
            superseded: 0,
            last_error: result.status.last_error,
          },
        };
      });
    } catch {
      /* A read-only status refresh must not replace a foreground operation's result. */
    }
  }, [assetApp, connection?.id]);

  const handleExportCandidate = async (kind: "skill" | "rule") => {
    const revision = ++requestRevision.current;
    setBusy("export");
    setError(null);
    setCandidateExport(null);
    try {
      const selected = await openNative({
        directory: kind === "skill",
        multiple: false,
        ...(kind === "rule"
          ? { filters: [{ name: "Markdown", extensions: ["md"] }] }
          : {}),
      });
      if (revision !== requestRevision.current || typeof selected !== "string")
        return;
      const output = await saveNative({
        defaultPath: await join(
          await downloadDir(),
          "candidate.nexusops-asset.json",
        ),
        filters: [
          { name: "NexusOps asset", extensions: ["nexusops-asset.json"] },
        ],
      });
      if (revision !== requestRevision.current || !output) return;
      const root = kind === "skill" ? selected : await dirname(selected);
      const entry = kind === "rule" ? await basename(selected) : null;
      if (revision !== requestRevision.current) return;
      const result = await exportTeamAICandidate({ kind, root, entry, output });
      if (revision === requestRevision.current) setCandidateExport(result);
    } catch (reason) {
      if (revision === requestRevision.current) setError(commandError(reason));
    } finally {
      if (revision === requestRevision.current) setBusy(null);
    }
  };

  const handleRestore = async (assetId: number, restoreApp: string) => {
    const revision = ++requestRevision.current;
    setBusy("restore");
    setError(null);
    let recoveryWarning: TeamCommandError | null = null;
    try {
      const restored = await restoreTeamBackup(restoreApp, assetId);
      if (revision !== requestRevision.current) return;
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
      if (revision !== requestRevision.current) return;
      setError(commandError(reason));
      setBusy(null);
      return;
    }

    try {
      const history = await getTeamLocalHistory(restoreApp);
      if (revision !== requestRevision.current) return;
      setLocalHistory(history);
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
      if (revision !== requestRevision.current) return;
      setError({
        code: "restore_refresh_failed",
        message: t("team.errorActions.restore_refresh_failed"),
      });
    } finally {
      if (revision === requestRevision.current) setBusy(null);
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

  function tracked<A extends unknown[]>(
    operation: (...args: A) => Promise<void>,
  ) {
    return (...args: A): Promise<void> => {
      if (cancelling.current) return Promise.resolve();
      setOperationNotice(null);
      const pending = operation(...args);
      pendingOperations.current.add(pending);
      void pending
        .finally(() => pendingOperations.current.delete(pending))
        .catch(() => {});
      return pending;
    };
  }

  const cancelCurrentOperation = async () => {
    if (
      cancelling.current ||
      busy === null ||
      busy === "disconnect" ||
      busy === "reporting"
    )
      return;
    cancelling.current = true;
    const revision = ++requestRevision.current;
    const target = assetApp;
    const pending = [...pendingOperations.current];
    setBusy("cancelling");
    setError(null);
    setOperationNotice(null);
    setMemberKey("");
    for (const read of fileReads.current) read.abort();
    let notice: "stopped" | "cancelFailed" | "recoveryFailed" = "stopped";
    try {
      try {
        await cancelTeamOperation();
      } catch {
        notice = "cancelFailed";
      }
      // Rust cancellation is a signal, not a rollback. Do not unlock the UI until
      // every owned call settles, including non-interruptible atomic writes.
      await Promise.allSettled(pending);
      if (revision !== requestRevision.current) return;
      try {
        await waitForTeamIdle();
        if (revision !== requestRevision.current) return;
        const status = await getTeamStatus();
        const history = status ? await getTeamLocalHistory(target) : [];
        if (revision !== requestRevision.current) return;
        setConnection(status);
        setLocalHistory(history);
        setLocalHistoryApp(target);
      } catch {
        notice = "recoveryFailed";
      }
      if (revision !== requestRevision.current) return;
      setProviderPreview(null);
      setSyncPlan(null);
      setSyncResult(null);
      setOverwriteAssets(new Set());
      setPendingRestoreAsset(null);
      setImportedProfileName(null);
      setOperationNotice(notice);
    } finally {
      cancelling.current = false;
      if (revision === requestRevision.current) setBusy(null);
    }
  };

  return {
    connection,
    error,
    busy,
    handleConnect: tracked(handleConnect),
    handleProfileFile: tracked(handleProfileFile),
    importedProfileName,
    gateway,
    setGateway,
    memberKey,
    setMemberKey,
    statusLabel,
    runRefresh: tracked(runRefresh),
    setShowDisconnect,
    showDisconnect,
    removeProviderCredentials,
    setRemoveProviderCredentials,
    handleDisconnect: tracked(handleDisconnect),
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
    handleProviderPreview: tracked(handleProviderPreview),
    selectedModel,
    handleProviderApply: tracked(handleProviderApply),
    handleProviderActivate: tracked(handleProviderActivate),
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
    handleSyncPreview: tracked(handleSyncPreview),
    syncPlan,
    handleSync: tracked(handleSync),
    pendingRestoreAsset,
    handleRestore: tracked(handleRestore),
    syncResult,
    overwriteAssets,
    setOverwrite,
    localHistoryApp,
    localHistory,
    operationNotice,
    cancelCurrentOperation,
    setReportingBusy,
    acknowledgements,
    teamaiSync,
    legacySync,
    handleRetryAcknowledgements: tracked(handleRetryAcknowledgements),
    refreshAcknowledgements,
    candidateExport,
    handleExportCandidate: tracked(handleExportCandidate),
  };
}
export type TeamWorkspaceModel = ReturnType<typeof useTeamWorkspace>;
