import { useTranslation } from "react-i18next";
import {
  Building2,
  KeyRound,
  Loader2,
  LogOut,
  Package,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";

import { cn } from "@/lib/utils";

import { Meta } from "./TeamFeedback";
import { type TeamWorkspaceModel } from "./useTeamWorkspace";
type Props = Pick<
  TeamWorkspaceModel,
  | "connection"
  | "statusLabel"
  | "runRefresh"
  | "busy"
  | "setShowDisconnect"
  | "showDisconnect"
  | "removeProviderCredentials"
  | "setRemoveProviderCredentials"
  | "handleDisconnect"
  | "models"
  | "lastSuccessfulSync"
>;
export function TeamConnectionSection({
  connection,
  statusLabel,
  runRefresh,
  busy,
  setShowDisconnect,
  showDisconnect,
  removeProviderCredentials,
  setRemoveProviderCredentials,
  handleDisconnect,
  models,
  lastSuccessfulSync,
}: Props) {
  const { t } = useTranslation();
  if (!connection) return null;
  return (
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
                  connection.status === "connected" ? "default" : "secondary"
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
          <p className="text-sm font-medium">{t("team.disconnect.title")}</p>
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
  );
}
