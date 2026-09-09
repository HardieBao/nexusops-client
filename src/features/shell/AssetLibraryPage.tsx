import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import type { ClientView } from "./navigation";

export function AssetLibraryPage({
  teamEnabled,
  onNavigate,
}: {
  teamEnabled: boolean;
  onNavigate: (view: ClientView) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="mx-auto max-w-5xl px-6 py-7">
      <p className="mb-7 max-w-2xl text-sm leading-6 text-muted-foreground">
        {t("clientHome.assetsHint")}
      </p>
      {teamEnabled && (
        <section className="mb-8 flex flex-wrap items-center justify-between gap-4 rounded-xl border bg-card p-5">
          <div>
            <h2 className="font-semibold">
              {t("clientNavigation.teamAssets")}
            </h2>
            <p className="mt-2 text-sm text-muted-foreground">
              {t("clientHome.teamAssetsHint")}
            </p>
          </div>
          <Button variant="outline" onClick={() => onNavigate("team")}>
            {t("clientNavigation.teamAssets")}
          </Button>
        </section>
      )}
      <h2 className="mb-3 font-semibold">
        {t("clientNavigation.personalAssets")}
      </h2>
      {(["skills", "prompts", "agents"] as const).map((view) => (
        <div
          key={view}
          className="flex items-center justify-between gap-4 border-b py-5"
        >
          <span>
            {t(
              view === "prompts" ? "clientNavigation.prompts" : `${view}.title`,
            )}
          </span>
          <Button variant="outline" onClick={() => onNavigate(view)}>
            {t("clientNavigation.open")}
          </Button>
        </div>
      ))}
      <Button
        className="mt-6"
        variant="ghost"
        onClick={() => onNavigate("skillsDiscovery")}
      >
        {t("clientNavigation.skillsDiscovery")}
      </Button>
    </div>
  );
}
