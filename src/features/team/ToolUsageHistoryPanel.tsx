import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  clearToolUsageHistory,
  getToolUsageHistory,
  getToolObservation,
  type ToolObservation,
  type ToolUsageDay,
} from "./api";
import { useSharedTeamWorkspace } from "./TeamWorkspaceBoundary";

export function ToolUsageHistoryPanel() {
  const workspace = useSharedTeamWorkspace();
  const { t } = useTranslation();
  if (!workspace) return null;
  if (!workspace.connection)
    return (
      <p className="mb-6 text-sm text-muted-foreground">
        {t("team.usageHistory.connect")}
      </p>
    );
  return (
    <History
      key={`${workspace.connection.id}:${workspace.connection.profile.key.id}`}
      disabled={workspace.busy !== null}
      onBusyChange={workspace.setReportingBusy}
    />
  );
}

function History({
  disabled,
  onBusyChange,
}: {
  disabled: boolean;
  onBusyChange: (busy: boolean) => void;
}) {
  const { t } = useTranslation();
  const [rows, setRows] = useState<ToolUsageDay[] | null>(null);
  const [tools, setTools] = useState<ToolObservation[]>([]);
  const [loading, setLoading] = useState(true);
  const [clearing, setClearing] = useState(false);
  const [error, setError] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [runtime, setRuntime] = useState("all");
  const revision = useRef(0);
  const active = useRef(true);
  const working = useRef(false);
  async function refresh() {
    const current = ++revision.current;
    setLoading(true);
    setError(false);
    try {
      const [result, observations] = await Promise.all([
        getToolUsageHistory(),
        getToolObservation(),
      ]);
      if (active.current && current === revision.current) {
        setRows(result);
        setTools(observations);
      }
    } catch {
      if (active.current && current === revision.current) {
        setError(true);
        setRows(null);
        setTools([]);
      }
    } finally {
      if (active.current && current === revision.current) setLoading(false);
    }
  }
  useEffect(() => {
    active.current = true;
    void refresh();
    return () => {
      active.current = false;
      revision.current++;
    };
  }, []);
  async function clear() {
    if (disabled || working.current) return;
    working.current = true;
    setClearing(true);
    onBusyChange(true);
    setError(false);
    try {
      await clearToolUsageHistory();
      if (active.current) {
        setConfirm(false);
        await refresh();
      }
    } catch {
      if (active.current) setError(true);
    } finally {
      working.current = false;
      onBusyChange(false);
      if (active.current) setClearing(false);
    }
  }
  const filtered =
    rows?.filter((row) => runtime === "all" || row.runtime === runtime) ?? [];
  return (
    <section
      className="mb-8 space-y-4 border-b pb-6"
      aria-busy={loading || clearing}
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-lg font-semibold">
          {t("team.usageHistory.title")}
        </h2>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            disabled={disabled || loading || clearing}
            onClick={() => void refresh()}
          >
            {t("common.refresh")}
          </Button>
          <Button
            variant="outline"
            disabled={disabled || loading || clearing || !rows?.length}
            onClick={() => setConfirm(true)}
          >
            {t("team.usageHistory.clear")}
          </Button>
        </div>
      </div>
      <p className="text-sm leading-6 text-muted-foreground">
        {t("team.usageHistory.description")}
      </p>
      {tools.length > 0 && !loading && (
        <div className="space-y-2 text-sm">
          {tools.map((tool) => (
            <p key={tool.runtime} className="flex flex-wrap gap-x-4 gap-y-1">
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
            </p>
          ))}
        </div>
      )}
      {confirm && (
        <div className="space-y-3 rounded-lg border p-4">
          <p>{t("team.usageHistory.clearNotice")}</p>
          <div className="flex gap-2">
            <Button
              variant="destructive"
              disabled={disabled || clearing}
              onClick={() => void clear()}
            >
              {t("team.usageHistory.confirmClear")}
            </Button>
            <Button
              variant="ghost"
              disabled={clearing}
              onClick={() => setConfirm(false)}
            >
              {t("common.cancel")}
            </Button>
          </div>
        </div>
      )}
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {t("team.usageHistory.error")}
        </p>
      )}
      {loading ? (
        <p role="status">{t("team.usageHistory.loading")}</p>
      ) : (
        rows && (
          <>
            <label className="flex items-center gap-3 text-sm">
              {t("team.usageHistory.runtime")}
              <select
                value={runtime}
                onChange={(event) => setRuntime(event.target.value)}
                className="rounded-md border bg-background px-3 py-2"
              >
                <option value="all">{t("team.usageHistory.all")}</option>
                <option value="codex">Codex</option>
                <option value="claude-code">Claude Code</option>
              </select>
            </label>
            {filtered.length ? (
              <div className="overflow-x-auto">
                <table className="w-full text-left text-sm">
                  <thead>
                    <tr>
                      {["day", "runtime", "event", "count", "observed"].map(
                        (column) => (
                          <th
                            key={column}
                            className="border-b px-3 py-2 font-medium"
                          >
                            {t(`team.usageHistory.${column}`)}
                          </th>
                        ),
                      )}
                    </tr>
                  </thead>
                  <tbody>
                    {filtered.map((row) => (
                      <tr key={`${row.day}:${row.runtime}:${row.event}`}>
                        <td className="whitespace-nowrap px-3 py-2">
                          {row.day}
                        </td>
                        <td className="px-3 py-2">
                          {row.runtime === "codex" ? "Codex" : "Claude Code"}
                        </td>
                        <td className="px-3 py-2">
                          {t(
                            `team.usageHistory.events.${row.event.replace(/\./g, "_")}`,
                            { defaultValue: row.event },
                          )}
                        </td>
                        <td className="px-3 py-2 tabular-nums">{row.count}</td>
                        <td className="whitespace-nowrap px-3 py-2">
                          {row.last_observed_at
                            .replace("T", " ")
                            .replace(/\+00:00$/, " UTC")}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <p className="text-sm text-muted-foreground">
                {t("team.usageHistory.empty")}
              </p>
            )}
          </>
        )
      )}
    </section>
  );
}
