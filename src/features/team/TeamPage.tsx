import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  Building2,
  CheckCircle2,
  KeyRound,
  Link2,
  Loader2,
  LogOut,
  Package,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";

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

const PROVIDER_APPS = ["claude", "codex", "gemini", "grokbuild", "opencode"];
const NO_MODEL = "__gateway_default__";

type BusyAction =
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

export function TeamPage() {
  const { t } = useTranslation();
  const [connection, setConnection] = useState<TeamConnection | null>(null);
  const [manifest, setManifest] = useState<TeamManifest | null>(null);
  const [gateway, setGateway] = useState("");
  const [memberKey, setMemberKey] = useState("");
  const [importedProfileName, setImportedProfileName] = useState<string | null>(
    null,
  );
  const [providerApp, setProviderApp] = useState("codex");
  const [assetApp, setAssetApp] = useState("codex");
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
  const connected = connection !== null;

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
        if (!active) return;
        setConnection(status);
        if (status) {
          setGateway(status.gateway_url);
          return previewTeamSync("codex").then((plan) => {
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
        void getTeamStatus().then((status) => {
          if (active) setConnection(status);
        });
      });
    return () => {
      active = false;
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
    if (!gateway.trim() || !memberKey.trim()) return;
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
    setError(null);
    setImportedProfileName(null);
    try {
      if (file.size > 1_048_576) throw new Error("Team Profile exceeds 1 MiB");
      const value = JSON.parse(await readTextFile(file)) as Record<
        string,
        unknown
      >;
      if (
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
    } catch (reason) {
      setGateway("");
      setMemberKey("");
      setError({
        code: "invalid_profile_file",
        message: reason instanceof Error ? reason.message : String(reason),
      });
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

  if (!connected) {
    return (
      <main className="h-full overflow-y-auto px-6 py-8">
        <section className="mx-auto grid min-h-[calc(100vh-11rem)] max-w-5xl items-center gap-10 lg:grid-cols-[1.1fr_0.9fr]">
          <div>
            <div className="mb-5 inline-flex items-center gap-2 rounded-full border bg-muted/40 px-3 py-1 text-xs font-medium text-muted-foreground">
              <ShieldCheck className="h-3.5 w-3.5" />
              {t("team.connect.eyebrow")}
            </div>
            <h2 className="max-w-2xl text-3xl font-semibold tracking-tight">
              {t("team.connect.heading")}
            </h2>
            <p className="mt-4 max-w-xl text-sm leading-6 text-muted-foreground">
              {t("team.connect.description")}
            </p>
            <div className="mt-8 grid gap-4 text-sm sm:grid-cols-3 lg:grid-cols-1">
              {["identity", "providers", "assets"].map((item) => (
                <div key={item} className="flex items-start gap-3">
                  <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
                  <span>{t(`team.connect.points.${item}`)}</span>
                </div>
              ))}
            </div>
          </div>

          <form
            onSubmit={handleConnect}
            className="rounded-2xl border bg-card p-6 shadow-sm"
          >
            <div className="mb-6 flex items-center gap-3">
              <div className="rounded-xl bg-primary/10 p-2.5 text-primary">
                <Link2 className="h-5 w-5" />
              </div>
              <div>
                <h3 className="font-semibold">{t("team.connect.formTitle")}</h3>
                <p className="text-xs text-muted-foreground">
                  {t("team.connect.formHint")}
                </p>
              </div>
            </div>
            <div className="space-y-5">
              <div className="space-y-2">
                <Label htmlFor="team-profile-file">
                  {t("team.connect.profileFile")}
                </Label>
                <Input
                  id="team-profile-file"
                  type="file"
                  accept="application/json,.json"
                  onChange={(event) => void handleProfileFile(event)}
                  disabled={busy !== null}
                />
                <p className="text-xs leading-5 text-muted-foreground">
                  {importedProfileName
                    ? t("team.connect.profileReady", {
                        name: importedProfileName,
                      })
                    : t("team.connect.profileHint")}
                </p>
              </div>
              <div className="space-y-2">
                <Label htmlFor="team-gateway">
                  {t("team.connect.gateway")}
                </Label>
                <Input
                  id="team-gateway"
                  type="url"
                  inputMode="url"
                  value={gateway}
                  onChange={(event) => setGateway(event.target.value)}
                  placeholder="https://gateway.example.com"
                  autoCapitalize="none"
                  autoCorrect="off"
                  disabled={busy !== null}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="team-key">{t("team.connect.key")}</Label>
                <Input
                  id="team-key"
                  type="password"
                  value={memberKey}
                  onChange={(event) => setMemberKey(event.target.value)}
                  placeholder={t("team.connect.keyPlaceholder")}
                  autoComplete="off"
                  disabled={busy !== null}
                />
                <p className="text-xs leading-5 text-muted-foreground">
                  {t("team.connect.keyHint")}
                </p>
              </div>
              {error && <ErrorAlert error={error} />}
              <Button
                type="submit"
                className="w-full"
                disabled={busy !== null || !gateway.trim() || !memberKey.trim()}
              >
                {busy === "connect" && (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                )}
                {t("team.connect.submit")}
              </Button>
            </div>
          </form>
        </section>
      </main>
    );
  }

  return (
    <main className="h-full overflow-y-auto px-6 py-6">
      <div className="mx-auto max-w-6xl space-y-6 pb-12">
        <section className="rounded-2xl border bg-card p-5 shadow-sm">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="flex min-w-0 items-start gap-3">
              <div className="rounded-xl bg-primary/10 p-2.5 text-primary">
                <Building2 className="h-5 w-5" />
              </div>
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <h2 className="truncate text-lg font-semibold">
                    {connection.profile.name}
                  </h2>
                  <Badge
                    variant={
                      connection.status === "connected"
                        ? "default"
                        : "secondary"
                    }
                  >
                    {statusLabel}
                  </Badge>
                </div>
                <p className="mt-1 truncate text-sm text-muted-foreground">
                  {connection.gateway_url}
                </p>
                <p className="mt-2 text-xs text-muted-foreground">
                  {t("team.connection.scope", {
                    organization: connection.profile.organization_id,
                    workspace: connection.profile.workspace_id,
                  })}
                </p>
              </div>
            </div>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => void runRefresh()}
                disabled={busy !== null}
              >
                <RefreshCw
                  className={cn(
                    "mr-2 h-4 w-4",
                    busy === "refresh" && "animate-spin",
                  )}
                />
                {t("team.actions.refresh")}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setShowDisconnect(true)}
                disabled={busy !== null}
              >
                <LogOut className="mr-2 h-4 w-4" />
                {t("team.actions.disconnect")}
              </Button>
            </div>
          </div>
          {showDisconnect && (
            <div className="mt-5 rounded-xl border border-amber-500/30 bg-amber-500/5 p-4">
              <p className="text-sm font-medium">
                {t("team.disconnect.title")}
              </p>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                {t("team.disconnect.description")}
              </p>
              <label className="mt-3 flex cursor-pointer items-start gap-2 text-sm">
                <Checkbox
                  checked={removeProviderCredentials}
                  onCheckedChange={(checked) =>
                    setRemoveProviderCredentials(checked === true)
                  }
                  disabled={busy !== null}
                  className="mt-0.5"
                />
                <span>{t("team.disconnect.scrubProviders")}</span>
              </label>
              <div className="mt-4 flex gap-2">
                <Button
                  size="sm"
                  variant="destructive"
                  onClick={() => void handleDisconnect()}
                  disabled={busy !== null}
                >
                  {busy === "disconnect" && (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  )}
                  {t("team.disconnect.confirm")}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setShowDisconnect(false)}
                  disabled={busy !== null}
                >
                  {t("common.cancel")}
                </Button>
              </div>
            </div>
          )}
          <div className="mt-5 grid gap-3 border-t pt-4 text-sm sm:grid-cols-2 lg:grid-cols-4">
            <Meta
              label={t("team.connection.key")}
              value={`${connection.profile.key.name} ····${connection.profile.key.last_four}`}
              icon={<KeyRound />}
            />
            <Meta
              label={t("team.connection.models")}
              value={String(models.length)}
              icon={<ShieldCheck />}
            />
            <Meta
              label={t("team.connection.checked")}
              value={new Date(connection.last_checked_at).toLocaleString()}
              icon={<RefreshCw />}
            />
            <Meta
              label={t("team.connection.lastSync")}
              value={lastSuccessfulSync}
              icon={<Package />}
            />
          </div>
          <p className="mt-4 text-xs leading-5 text-muted-foreground">
            {t("team.connection.usageHint")}
          </p>
        </section>

        {error && <ErrorAlert error={error} />}

        <section className="grid gap-6 lg:grid-cols-[0.9fr_1.1fr]">
          <div className="rounded-2xl border bg-card p-5 shadow-sm">
            <div className="mb-5">
              <h3 className="font-semibold">{t("team.provider.title")}</h3>
              <p className="mt-1 text-sm leading-5 text-muted-foreground">
                {t("team.provider.description")}
              </p>
            </div>
            <div className="space-y-4">
              <div className="space-y-2">
                <Label>{t("team.provider.app")}</Label>
                <Select
                  value={providerApp}
                  disabled={busy !== null}
                  onValueChange={(value) => {
                    requestRevision.current += 1;
                    setProviderApp(value);
                    setProviderPreview(null);
                  }}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {providerApps.map((item) => (
                      <SelectItem key={item} value={item}>
                        {t(`apps.${item}`)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-2">
                <Label>{t("team.provider.model")}</Label>
                <Select
                  value={model}
                  disabled={busy !== null}
                  onValueChange={(value) => {
                    requestRevision.current += 1;
                    setModel(value);
                    setProviderPreview(null);
                  }}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={NO_MODEL} disabled>
                      {t("team.provider.noAuthorizedModel")}
                    </SelectItem>
                    {models.map((item) => (
                      <SelectItem key={item.id} value={item.id}>
                        {item.name || item.id}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <Alert>
                <KeyRound className="h-4 w-4" />
                <AlertTitle>{t("team.provider.credentialTitle")}</AlertTitle>
                <AlertDescription>
                  {t("team.provider.credentialHint")}
                </AlertDescription>
              </Alert>
              {providerPreview && (
                <div className="rounded-xl bg-muted/50 p-4 text-sm">
                  <div className="flex items-center justify-between gap-3">
                    <span className="font-medium">{providerPreview.name}</span>
                    <Badge variant="secondary">
                      {t(`team.provider.change.${providerPreview.change}`)}
                    </Badge>
                  </div>
                  <p className="mt-2 break-all font-mono text-xs text-muted-foreground">
                    {providerPreview.base_url}
                  </p>
                  {providerPreview.changed_fields.length > 0 && (
                    <p className="mt-2 text-xs text-muted-foreground">
                      {t("team.provider.changed", {
                        fields: providerPreview.changed_fields.join(", "),
                      })}
                    </p>
                  )}
                </div>
              )}
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  onClick={() => void handleProviderPreview()}
                  disabled={
                    busy !== null ||
                    connection.status !== "connected" ||
                    providerApps.length === 0 ||
                    selectedModel === null
                  }
                >
                  {busy === "provider" && (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  )}
                  {t("team.provider.preview")}
                </Button>
                <Button
                  onClick={() => void handleProviderApply()}
                  disabled={
                    busy !== null ||
                    connection.status !== "connected" ||
                    providerPreview === null ||
                    providerPreview.change === "unchanged"
                  }
                >
                  {providerPreview?.change === "update_requires_confirmation"
                    ? t("team.provider.confirmUpdate")
                    : t("team.provider.import")}
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => void handleProviderActivate()}
                  disabled={
                    busy !== null ||
                    connection.status !== "connected" ||
                    providerPreview?.change !== "unchanged" ||
                    providerPreview.active
                  }
                >
                  {providerPreview?.active
                    ? t("team.provider.active")
                    : t("team.provider.activate")}
                </Button>
              </div>
            </div>
          </div>

          <div className="rounded-2xl border bg-card p-5 shadow-sm">
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <h3 className="font-semibold">{t("team.assets.title")}</h3>
                <p className="mt-1 text-sm leading-5 text-muted-foreground">
                  {t("team.assets.description")}
                </p>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <Select
                  value={assetApp}
                  disabled={busy !== null}
                  onValueChange={(value) => {
                    requestRevision.current += 1;
                    setAssetApp(value);
                    setSyncPlan(null);
                    setSyncResult(null);
                    setOverwriteAssets(new Set());
                    setLastSyncAt(null);
                    setPendingRestoreAsset(null);
                    setLocalHistory([]);
                    setLocalHistoryApp(value);
                  }}
                >
                  <SelectTrigger
                    className="h-8 w-36"
                    aria-label={t("team.assets.app")}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {PROVIDER_APPS.map((item) => (
                      <SelectItem key={item} value={item}>
                        {t(`apps.${item}`)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <Badge variant="outline">
                  {t("team.assets.count", {
                    count: manifest?.assets.length ?? 0,
                  })}
                </Badge>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => void handleSyncPreview()}
                  disabled={busy !== null || connection.status !== "connected"}
                >
                  {busy === "sync" && !syncPlan && (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  )}
                  {t("team.assets.preview")}
                </Button>
                <Button
                  size="sm"
                  onClick={() => void handleSync()}
                  disabled={busy !== null || syncPlan === null}
                >
                  {busy === "sync" && syncPlan && (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  )}
                  {t("team.assets.sync")}
                </Button>
              </div>
            </div>

            {manifest?.conflicts.length ? (
              <Alert variant="destructive" className="mt-5">
                <AlertTriangle className="h-4 w-4" />
                <AlertTitle>{t("team.assets.conflicts")}</AlertTitle>
                <AlertDescription>
                  {manifest.conflicts
                    .map((conflict) => conflict.slug)
                    .join(", ")}
                </AlertDescription>
              </Alert>
            ) : null}

            {pendingRestoreAsset && (
              <div className="mt-5 rounded-xl border border-amber-500/30 bg-amber-500/5 p-4">
                <p className="text-sm font-medium">
                  {t("team.assets.restoreConfirmTitle", {
                    name: pendingRestoreAsset.name,
                  })}
                </p>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">
                  {t("team.assets.restoreConfirmDescription")}{" "}
                  {t("team.assets.restoreConfirmTarget", {
                    app: t(`apps.${pendingRestoreAsset.app}`),
                  })}
                </p>
                <div className="mt-3 flex gap-2">
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={() =>
                      void handleRestore(
                        pendingRestoreAsset.id,
                        pendingRestoreAsset.app,
                      )
                    }
                    disabled={busy !== null}
                  >
                    {busy === "restore" && (
                      <Loader2 className="mr-2 h-3.5 w-3.5 animate-spin" />
                    )}
                    {t("team.assets.restoreConfirm")}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => setPendingRestoreAsset(null)}
                    disabled={busy !== null}
                  >
                    {t("common.cancel")}
                  </Button>
                </div>
              </div>
            )}

            <div className="mt-5 divide-y rounded-xl border">
              {!manifest?.assets.length ? (
                <div className="px-4 py-10 text-center">
                  <Package className="mx-auto h-6 w-6 text-muted-foreground" />
                  <p className="mt-3 text-sm font-medium">
                    {t("team.assets.empty")}
                  </p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {t("team.assets.emptyHint")}
                  </p>
                </div>
              ) : (
                manifest.assets.map((asset) => {
                  const planItem = syncPlan?.items.find(
                    (item) => item.asset.asset_id === asset.asset_id,
                  );
                  const result = syncResult?.items.find(
                    (item) => item.asset_id === asset.asset_id,
                  );
                  const hasLocalConflict =
                    !planItem?.inspection_error_message &&
                    (planItem?.drift === "local_modified" ||
                      planItem?.drift === "both_modified");
                  return (
                    <article
                      key={`${asset.asset_id}:${asset.revision}`}
                      className="grid gap-3 px-4 py-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center"
                    >
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2">
                          <h4 className="truncate text-sm font-medium">
                            {asset.name}
                          </h4>
                          <Badge
                            variant="secondary"
                            className="text-[10px] uppercase"
                          >
                            {asset.kind}
                          </Badge>
                        </div>
                        <p className="mt-1 truncate font-mono text-[11px] text-muted-foreground">
                          {asset.slug} · r{asset.revision} ·{" "}
                          {asset.content_hash.slice(0, 12)}
                        </p>
                        {planItem && (
                          <p className="mt-1 truncate text-[11px] text-muted-foreground">
                            {t(`team.assets.support.${planItem.support}`)}
                          </p>
                        )}
                        {result?.error_message && (
                          <p className="mt-1 text-xs text-destructive">
                            {t(
                              `team.errorActions.${result.error_code ?? "unknown"}`,
                              {
                                defaultValue: result.error_message,
                              },
                            )}
                          </p>
                        )}
                        {!result?.error_message &&
                          planItem?.inspection_error_message && (
                            <p className="mt-1 text-xs text-destructive">
                              {t(
                                `team.errorActions.${planItem.inspection_error_code ?? "local_state"}`,
                                {
                                  defaultValue:
                                    planItem.inspection_error_message,
                                },
                              )}
                            </p>
                          )}
                      </div>
                      <div className="flex items-center justify-between gap-3 sm:justify-end">
                        {planItem?.has_backup && (
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() =>
                              setPendingRestoreAsset({
                                id: asset.asset_id,
                                name: asset.name,
                                app: assetApp,
                              })
                            }
                            disabled={busy !== null}
                          >
                            {busy === "restore" && (
                              <Loader2 className="mr-2 h-3.5 w-3.5 animate-spin" />
                            )}
                            {t("team.assets.restoreBackup")}
                          </Button>
                        )}
                        {hasLocalConflict && (
                          <label className="flex cursor-pointer items-center gap-2 text-xs">
                            <Checkbox
                              checked={overwriteAssets.has(asset.asset_id)}
                              onCheckedChange={(checked) =>
                                setOverwrite(asset.asset_id, checked === true)
                              }
                              disabled={busy !== null}
                            />
                            {t("team.assets.allowOverwrite")}
                          </label>
                        )}
                        <Badge
                          variant={
                            result?.outcome === "failed"
                              ? "destructive"
                              : "secondary"
                          }
                        >
                          {result
                            ? t(`team.assets.outcome.${result.outcome}`)
                            : planItem?.inspection_error_message
                              ? t("team.assets.inspectFailed")
                              : planItem
                                ? t(`team.assets.drift.${planItem.drift}`)
                                : t("team.assets.available")}
                        </Badge>
                      </div>
                    </article>
                  );
                })
              )}
            </div>
            {syncPlan && syncPlan.withdrawn.length > 0 && (
              <div className="mt-4 rounded-xl border border-dashed">
                <p className="border-b px-4 py-2 text-xs font-medium text-muted-foreground">
                  {t("team.assets.withdrawnTitle")}
                </p>
                <div className="divide-y">
                  {syncPlan.withdrawn.map((asset) => (
                    <article
                      key={`withdrawn:${asset.asset_id}`}
                      className="flex items-center justify-between gap-3 px-4 py-3"
                    >
                      <div className="min-w-0">
                        <p className="truncate text-sm font-medium">
                          {asset.name}
                        </p>
                        <p className="mt-1 text-xs text-muted-foreground">
                          {asset.kind ?? t("team.assets.unknownKind")} · r
                          {asset.revision}
                          {asset.has_backup
                            ? ` · ${t("team.assets.backupAvailable")}`
                            : ""}
                        </p>
                        {asset.recovery_error && (
                          <p className="mt-1 text-xs text-destructive">
                            {t("team.errorActions.upstream_pending")}
                          </p>
                        )}
                      </div>
                      <div className="flex items-center gap-2">
                        {asset.has_backup && (
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() =>
                              setPendingRestoreAsset({
                                id: asset.asset_id,
                                name: asset.name,
                                app: assetApp,
                              })
                            }
                            disabled={busy !== null}
                          >
                            {t("team.assets.restoreBackup")}
                          </Button>
                        )}
                        <Badge variant="outline">
                          {asset.local_file_present
                            ? t("team.assets.withdrawnPresent")
                            : t("team.assets.withdrawnMissing")}
                        </Badge>
                      </div>
                    </article>
                  ))}
                </div>
              </div>
            )}
            {!syncPlan &&
              localHistoryApp === assetApp &&
              localHistory.length > 0 && (
                <div className="mt-4 rounded-xl border border-dashed">
                  <div className="border-b px-4 py-3">
                    <p className="text-xs font-medium text-muted-foreground">
                      {t("team.assets.localHistoryTitle")}
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                      {t("team.assets.localHistoryHint")}
                    </p>
                  </div>
                  <div className="divide-y">
                    {localHistory.map((asset) => (
                      <article
                        key={`local:${asset.asset_id}`}
                        className="flex items-center justify-between gap-3 px-4 py-3"
                      >
                        <div className="min-w-0">
                          <p className="truncate text-sm font-medium">
                            {asset.name}
                          </p>
                          <p className="mt-1 text-xs text-muted-foreground">
                            {asset.kind ?? t("team.assets.unknownKind")} · r
                            {asset.revision}
                          </p>
                          {asset.recovery_error && (
                            <p className="mt-1 text-xs text-destructive">
                              {t("team.errorActions.upstream_pending")}
                            </p>
                          )}
                        </div>
                        <div className="flex items-center gap-2">
                          {asset.has_backup && (
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={() =>
                                setPendingRestoreAsset({
                                  id: asset.asset_id,
                                  name: asset.name,
                                  app: localHistoryApp,
                                })
                              }
                              disabled={busy !== null}
                            >
                              {t("team.assets.restoreBackup")}
                            </Button>
                          )}
                          <Badge variant="outline">
                            {asset.local_file_present
                              ? t("team.assets.withdrawnPresent")
                              : t("team.assets.withdrawnMissing")}
                          </Badge>
                        </div>
                      </article>
                    ))}
                  </div>
                </div>
              )}
            <p className="mt-4 text-xs leading-5 text-muted-foreground">
              {t("team.assets.localRetention")}
            </p>
          </div>
        </section>
      </div>
    </main>
  );
}

function Meta({
  label,
  value,
  icon,
}: {
  label: string;
  value: string;
  icon: React.ReactNode;
}) {
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      <span className="text-muted-foreground [&>svg]:h-4 [&>svg]:w-4">
        {icon}
      </span>
      <div className="min-w-0">
        <p className="text-xs text-muted-foreground">{label}</p>
        <p className="truncate font-medium">{value}</p>
      </div>
    </div>
  );
}

function ErrorAlert({ error }: { error: TeamCommandError }) {
  const { t } = useTranslation();
  return (
    <Alert variant="destructive">
      <AlertTriangle className="h-4 w-4" />
      <AlertTitle>
        {t(`team.errors.${error.code}`, {
          defaultValue: t("team.errors.title"),
        })}
      </AlertTitle>
      <AlertDescription>
        {t(`team.errorActions.${error.code}`, { defaultValue: error.message })}
      </AlertDescription>
    </Alert>
  );
}
