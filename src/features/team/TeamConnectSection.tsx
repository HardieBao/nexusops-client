import { useTranslation } from "react-i18next";
import { CheckCircle2, Link2, Loader2, ShieldCheck } from "lucide-react";

import { Button } from "@/components/ui/button";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

import { ErrorAlert } from "./TeamFeedback";
import { type TeamWorkspaceModel } from "./useTeamWorkspace";
type Props = Pick<
  TeamWorkspaceModel,
  | "handleConnect"
  | "handleProfileFile"
  | "busy"
  | "importedProfileName"
  | "gateway"
  | "setGateway"
  | "memberKey"
  | "setMemberKey"
  | "error"
>;
export function TeamConnectSection({
  handleConnect,
  handleProfileFile,
  busy,
  importedProfileName,
  gateway,
  setGateway,
  memberKey,
  setMemberKey,
  error,
}: Props) {
  const { t } = useTranslation();
  return (
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
            <Label htmlFor="team-gateway">{t("team.connect.gateway")}</Label>
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
  );
}
