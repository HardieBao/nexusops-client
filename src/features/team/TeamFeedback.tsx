import { useTranslation } from "react-i18next";
import { AlertTriangle } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

import { type TeamCommandError } from "./api";

export function Meta({
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

export function ErrorAlert({ error }: { error: TeamCommandError }) {
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
