import { useTranslation } from "react-i18next";
import { useEffect } from "react";
import { AlertTriangle, Loader2, Package } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

import { PROVIDER_APPS, type TeamWorkspaceModel } from "./useTeamWorkspace";
type Props = Pick<
  TeamWorkspaceModel,
  | "assetApp"
  | "busy"
  | "requestRevision"
  | "setAssetApp"
  | "setSyncPlan"
  | "setSyncResult"
  | "setOverwriteAssets"
  | "setLastSyncAt"
  | "setPendingRestoreAsset"
  | "setLocalHistory"
  | "setLocalHistoryApp"
  | "manifest"
  | "handleSyncPreview"
  | "connection"
  | "syncPlan"
  | "handleSync"
  | "pendingRestoreAsset"
  | "handleRestore"
  | "syncResult"
  | "overwriteAssets"
  | "setOverwrite"
  | "localHistoryApp"
  | "localHistory"
  | "acknowledgements"
  | "teamaiSync"
  | "legacySync"
  | "handleRetryAcknowledgements"
  | "refreshAcknowledgements"
>;
export function TeamAssetsSection({
  assetApp,
  busy,
  requestRevision,
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
  connection,
  syncPlan,
  handleSync,
  pendingRestoreAsset,
  handleRestore,
  syncResult,
  overwriteAssets,
  setOverwrite,
  localHistoryApp,
  localHistory,
  acknowledgements,
  teamaiSync,
  legacySync,
  handleRetryAcknowledgements,
  refreshAcknowledgements,
}: Props) {
  const { t } = useTranslation();
  useEffect(() => {
    if (!connection || busy !== null) return;
    void refreshAcknowledgements();
    const interval = window.setInterval(
      () => void refreshAcknowledgements(),
      15000,
    );
    return () => window.clearInterval(interval);
  }, [connection?.id, busy, refreshAcknowledgements]);
  if (!connection) return null;
  return (
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
          {(teamaiSync || acknowledgements) && (
            <Button
              size="sm"
              variant="ghost"
              disabled={busy !== null}
              onClick={() => void handleRetryAcknowledgements()}
            >
              {t("team.assets.retryAck")}
            </Button>
          )}
        </div>
      </div>

      {legacySync && (
        <p className="mt-3 text-xs text-muted-foreground">
          {t("team.assets.legacySync")}
        </p>
      )}
      {acknowledgements &&
        (acknowledgements.waiting > 0 ||
          acknowledgements.acknowledged > 0 ||
          acknowledgements.superseded > 0 ||
          acknowledgements.last_error) && (
          <Alert className="mt-4">
            <AlertTitle>{t("team.assets.ackTitle")}</AlertTitle>
            <AlertDescription>
              {t(
                acknowledgements.waiting > 0
                  ? "team.assets.ackWaiting"
                  : "team.assets.ackConfirmed",
                {
                  count:
                    acknowledgements.waiting || acknowledgements.acknowledged,
                },
              )}
              {acknowledgements.superseded > 0 && (
                <p>
                  {t("team.assets.ackSuperseded", {
                    count: acknowledgements.superseded,
                  })}
                </p>
              )}
              {acknowledgements.last_error && (
                <p>
                  {t(`team.errorActions.${acknowledgements.last_error}`, {
                    defaultValue: acknowledgements.last_error,
                  })}
                </p>
              )}
            </AlertDescription>
          </Alert>
        )}
      {manifest?.conflicts.length ? (
        <Alert variant="destructive" className="mt-5">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{t("team.assets.conflicts")}</AlertTitle>
          <AlertDescription>
            {manifest.conflicts.map((conflict) => conflict.slug).join(", ")}
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
            <p className="mt-3 text-sm font-medium">{t("team.assets.empty")}</p>
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
                  {planItem?.activation_path && (
                    <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">
                      {t("team.assets.activationPath", {
                        path: planItem.activation_path,
                      })}
                    </p>
                  )}
                  {!result?.error_message &&
                    planItem?.inspection_error_message && (
                      <p className="mt-1 text-xs text-destructive">
                        {t(
                          `team.errorActions.${planItem.inspection_error_code ?? "local_state"}`,
                          {
                            defaultValue: planItem.inspection_error_message,
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
                      result?.outcome === "failed" ? "destructive" : "secondary"
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
                  <p className="truncate text-sm font-medium">{asset.name}</p>
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
      {!syncPlan && localHistoryApp === assetApp && localHistory.length > 0 && (
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
                  <p className="truncate text-sm font-medium">{asset.name}</p>
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
                      ? t("team.assets.localFilePresent")
                      : t("team.assets.localFileMissing")}
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
  );
}
