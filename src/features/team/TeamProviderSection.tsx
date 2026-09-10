import { useTranslation } from "react-i18next";
import { KeyRound, Loader2 } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

import { NO_MODEL, type TeamWorkspaceModel } from "./useTeamWorkspace";
type Props = Pick<
  TeamWorkspaceModel,
  | "providerApp"
  | "busy"
  | "requestRevision"
  | "setProviderApp"
  | "setProviderPreview"
  | "providerApps"
  | "model"
  | "setModel"
  | "models"
  | "providerPreview"
  | "handleProviderPreview"
  | "connection"
  | "selectedModel"
  | "handleProviderApply"
  | "handleProviderActivate"
>;
export function TeamProviderSection({
  providerApp,
  busy,
  requestRevision,
  setProviderApp,
  setProviderPreview,
  providerApps,
  model,
  setModel,
  models,
  providerPreview,
  handleProviderPreview,
  connection,
  selectedModel,
  handleProviderApply,
  handleProviderActivate,
}: Props) {
  const { t } = useTranslation();
  if (!connection) return null;
  return (
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
  );
}
