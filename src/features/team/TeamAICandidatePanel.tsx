import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useSharedTeamWorkspace } from "./TeamWorkspaceBoundary";

export function TeamAICandidatePanel() {
  const workspace = useSharedTeamWorkspace();
  const [attempted, setAttempted] = useState(false);
  const { t } = useTranslation();
  if (!workspace) return null;
  return (
    <section className="mt-8 rounded-xl border bg-card p-5">
      <h2 className="font-semibold">{t("team.candidate.title")}</h2>
      <p className="mt-2 text-sm leading-6 text-muted-foreground">
        {t("team.candidate.description")}
      </p>
      <div className="mt-4 flex flex-wrap gap-2">
        {(["skill", "rule"] as const).map((kind) => (
          <Button
            key={kind}
            variant="outline"
            disabled={workspace.busy !== null}
            onClick={() => {
              setAttempted(true);
              void workspace.handleExportCandidate(kind);
            }}
          >
            {t(`team.candidate.${kind}`)}
          </Button>
        ))}
      </div>
      {attempted &&
        (workspace.busy === "export" || workspace.busy === "cancelling") && (
          <div className="mt-4 flex items-center gap-3">
            <p role="status" className="text-sm text-muted-foreground">
              {t("team.candidate.working")}
            </p>
            <Button
              variant="ghost"
              disabled={workspace.busy === "cancelling"}
              onClick={() => void workspace.cancelCurrentOperation()}
            >
              {t("common.cancel")}
            </Button>
          </div>
        )}
      {attempted && workspace.operationNotice && workspace.busy === null && (
        <p role="status" className="mt-3 text-sm text-muted-foreground">
          {t(`teamOperation.${workspace.operationNotice}`)}
        </p>
      )}
      {attempted && workspace.error && (
        <p role="alert" className="mt-3 text-sm text-destructive">
          {t(`team.errorActions.${workspace.error.code}`, {
            defaultValue: t("team.candidate.error"),
          })}
        </p>
      )}
      {attempted && workspace.candidateExport && (
        <div role="status" className="mt-4 space-y-2 text-sm">
          <p>
            {t("team.candidate.ready", {
              name: workspace.candidateExport.name,
            })}
          </p>
          <p className="break-all font-mono text-xs">
            {workspace.candidateExport.output}
          </p>
          <p className="text-muted-foreground">{t("team.candidate.next")}</p>
        </div>
      )}
    </section>
  );
}
