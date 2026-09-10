import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import type { TeamWorkspaceModel } from "./useTeamWorkspace";

export function TeamOperationStatus({
  busy,
  operationNotice,
  cancelCurrentOperation,
  runRefresh,
}: Pick<
  TeamWorkspaceModel,
  "busy" | "operationNotice" | "cancelCurrentOperation" | "runRefresh"
>) {
  const { t } = useTranslation();
  const canCancel =
    busy !== null &&
    busy !== "disconnect" &&
    busy !== "reporting" &&
    busy !== "cancelling";
  if (!canCancel && busy !== "cancelling" && !operationNotice) return null;
  return (
    <div className="mb-5 flex flex-wrap items-center justify-between gap-3 rounded-lg border bg-muted/40 px-4 py-3">
      <p
        role="status"
        className="max-w-2xl text-sm leading-6 text-muted-foreground"
      >
        {t(
          `teamOperation.${operationNotice ?? (busy === "cancelling" ? "settling" : "running")}`,
        )}
      </p>
      {(canCancel || busy === "cancelling") && (
        <Button
          variant="outline"
          disabled={!canCancel}
          onClick={() => void cancelCurrentOperation()}
        >
          {t(
            busy === "cancelling"
              ? "teamOperation.waiting"
              : "teamOperation.stop",
          )}
        </Button>
      )}
      {operationNotice && busy === null && (
        <Button variant="outline" onClick={() => void runRefresh()}>
          {t("team.actions.refresh")}
        </Button>
      )}
    </div>
  );
}
