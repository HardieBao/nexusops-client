import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Building2, Plug, ArrowRight, Library } from "lucide-react";
import { Button } from "@/components/ui/button";
import { APP_ICON_MAP } from "@/config/appConfig";
import type { AppId } from "@/lib/api/types";
import { getTeamStatus, getToolObservation } from "@/features/team/api";
import { useSharedTeamWorkspace } from "@/features/team/TeamWorkspaceBoundary";
import type { ClientView } from "./navigation";

interface Props {
  teamEnabled: boolean;
  activeApp: AppId;
  providerName?: string;
  onNavigate: (view: ClientView) => void;
}
export function ClientHomePage({
  teamEnabled,
  activeApp,
  providerName,
  onNavigate,
}: Props) {
  const { t } = useTranslation();
  const shared = useSharedTeamWorkspace();
  const team = useQuery({
    queryKey: ["team", "status"],
    queryFn: getTeamStatus,
    enabled: teamEnabled && !shared,
    staleTime: 0,
    retry: false,
    refetchOnWindowFocus: false,
  });
  const connection = shared ? shared.connection : team.data;
  const activity = useQuery({
    queryKey: [
      "team",
      "observation",
      connection?.id,
      connection?.profile.key.id,
    ],
    queryFn: getToolObservation,
    enabled: teamEnabled && !!connection && !shared?.busy,
    staleTime: 0,
    gcTime: 0,
    retry: false,
    refetchOnWindowFocus: false,
  });
  return (
    <div className="mx-auto max-w-5xl space-y-8 px-6 py-7">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h2 className="text-2xl font-semibold tracking-tight">
            {t("clientHome.title")}
          </h2>
          <p className="mt-3 max-w-2xl text-sm leading-6 text-muted-foreground">
            {t("clientHome.description")}
          </p>
        </div>
        <Button onClick={() => onNavigate("providers")}>
          {t("clientHome.configureTools")}
          <ArrowRight className="ml-2 h-4 w-4" />
        </Button>
      </div>
      <section
        className="rounded-xl border bg-card p-5"
        aria-labelledby="home-organization"
      >
        <div className="flex items-center gap-3">
          <Building2 className="h-5 w-5 text-muted-foreground" />
          <h3 id="home-organization" className="font-semibold">
            {t("clientHome.organization")}
          </h3>
        </div>
        {teamEnabled && !shared && team.isPending ? (
          <div
            aria-busy="true"
            className="mt-5 h-14 animate-pulse rounded bg-muted"
          />
        ) : connection ? (
          <>
            <p className="mt-4 break-words text-lg font-medium">
              {connection.profile.name}
            </p>
            <p className="mt-2 text-sm text-muted-foreground">
              {t("clientHome.lastVerification")}:{" "}
              {new Date(connection.last_checked_at).toLocaleString()}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t(`clientHome.status.${connection.status}`)}
            </p>
            <Button
              className="mt-4"
              variant="outline"
              onClick={() => onNavigate("team")}
            >
              {t("clientNavigation.team")}
            </Button>
          </>
        ) : (
          <>
            <p className="mt-4 text-sm leading-6 text-muted-foreground">
              {t(
                teamEnabled
                  ? "clientHome.connectHint"
                  : "clientNavigation.teamUnavailable",
              )}
            </p>
            {teamEnabled && (
              <Button
                className="mt-4"
                variant="outline"
                onClick={() => onNavigate("team")}
              >
                {t("clientHome.connect")}
              </Button>
            )}
          </>
        )}
        {teamEnabled && !shared && team.isError && (
          <p role="alert" className="mt-3 text-sm text-destructive">
            {t("clientHome.readError")}
          </p>
        )}
      </section>
      {teamEnabled && connection && (
        <section aria-labelledby="home-activity" className="space-y-3">
          <h3 id="home-activity" className="font-semibold">
            {t("team.usageHistory.title")}
          </h3>
          {activity.isPending ? (
            <p role="status" className="text-sm text-muted-foreground">
              {t("team.usageHistory.loading")}
            </p>
          ) : activity.isError ? (
            <p role="alert" className="text-sm text-destructive">
              {t("team.usageHistory.error")}
            </p>
          ) : (
            <ul className="divide-y">
              {activity.data?.map((tool) => (
                <li
                  key={tool.runtime}
                  className="flex flex-wrap gap-x-4 gap-y-1 py-3 text-sm"
                >
                  <strong>
                    {tool.runtime === "codex" ? "Codex" : "Claude Code"}
                  </strong>
                  <span>
                    {t(`team.usageHistory.registration.${tool.registration}`)}
                  </span>
                  <span>
                    {t(
                      `team.usageHistory.collection.${tool.collection_enabled ? "enabled" : "disabled"}`,
                    )}
                  </span>
                  <span>
                    {t(`team.usageHistory.observation.${tool.observation}`)}
                  </span>
                </li>
              ))}
            </ul>
          )}
          <div className="flex flex-wrap gap-2">
            <Button
              variant="outline"
              disabled={!!shared?.busy}
              onClick={() => onNavigate("team")}
            >
              {t("clientNavigation.team")}
            </Button>
            <Button
              variant="outline"
              disabled={!!shared?.busy}
              onClick={() => onNavigate("usage")}
            >
              {t("clientNavigation.usage")}
            </Button>
          </div>
        </section>
      )}
      <section aria-labelledby="home-tools">
        <h3 id="home-tools" className="mb-4 font-semibold">
          {t("clientHome.currentTool")}
        </h3>
        <div className="flex flex-wrap items-center justify-between gap-4 border-y py-5">
          <div className="flex items-center gap-4">
            <Plug className="h-5 w-5 text-muted-foreground" />
            <div>
              <p className="font-medium">{APP_ICON_MAP[activeApp].label}</p>
              <p className="mt-1 text-sm text-muted-foreground">
                {providerName || t("clientHome.noProvider")}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                {t("clientHome.configurationOnly")}
              </p>
            </div>
          </div>
          <Button variant="outline" onClick={() => onNavigate("providers")}>
            {t("clientHome.configureTools")}
          </Button>
        </div>
      </section>
      <section className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex items-start gap-4">
          <Library className="mt-1 h-5 w-5 text-muted-foreground" />
          <div>
            <h3 className="font-semibold">
              {t("clientNavigation.assetLibrary")}
            </h3>
            <p className="mt-2 text-sm text-muted-foreground">
              {t("clientHome.assetsHint")}
            </p>
          </div>
        </div>
        <Button variant="outline" onClick={() => onNavigate("assetLibrary")}>
          {t("clientHome.openAssets")}
        </Button>
      </section>
    </div>
  );
}
