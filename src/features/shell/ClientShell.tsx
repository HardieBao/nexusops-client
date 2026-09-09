import type { CSSProperties, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  LayoutDashboard,
  Plug,
  Library,
  BarChart3,
  Building2,
  Settings,
} from "lucide-react";
import logo from "@/assets/icons/app-icon.png";
import type { AppId } from "@/lib/api/types";
import {
  advancedViewsFor,
  mainViewFor,
  VIEW_TITLE_KEYS,
  type ClientView,
  type MainView,
} from "./navigation";
import "./shell.css";

interface Props {
  view: ClientView;
  activeApp: AppId;
  teamEnabled: boolean;
  busy: boolean;
  titlebarHeight: number;
  onNavigate: (view: ClientView) => void;
  children: ReactNode;
}
const primary = [
  { view: "home", icon: LayoutDashboard },
  { view: "providers", icon: Plug },
  { view: "assetLibrary", icon: Library },
  { view: "usage", icon: BarChart3 },
] as const;
const secondary = [
  { view: "team", icon: Building2 },
  { view: "settings", icon: Settings },
] as const;

export function ClientShell({
  view,
  activeApp,
  teamEnabled,
  busy,
  titlebarHeight,
  onNavigate,
  children,
}: Props) {
  const { t } = useTranslation();
  const selected = mainViewFor(view);
  const button = (item: { view: MainView; icon: typeof Settings }) => {
    const disabled = busy || (item.view === "team" && !teamEnabled);
    return (
      <button
        key={item.view}
        type="button"
        className="client-nav-button"
        aria-current={selected === item.view ? "page" : undefined}
        disabled={disabled}
        title={
          item.view === "team" && !teamEnabled
            ? t("clientNavigation.teamUnavailable")
            : t("clientNavigation.openPage", {
                page: t(VIEW_TITLE_KEYS[item.view]),
              })
        }
        aria-label={t(VIEW_TITLE_KEYS[item.view])}
        onClick={() => onNavigate(item.view)}
      >
        <item.icon aria-hidden="true" size={18} />
        <span className="client-nav-label">
          {t(VIEW_TITLE_KEYS[item.view])}
        </span>
      </button>
    );
  };
  return (
    <div
      className="client-shell"
      style={
        { "--client-titlebar-height": `${titlebarHeight}px` } as CSSProperties
      }
    >
      <aside
        className="client-sidebar"
        aria-label={t("clientNavigation.sidebar")}
      >
        <div className="client-brand">
          <img src={logo} alt="" />
          <span className="client-nav-label">NexusOps</span>
        </div>
        <nav aria-label={t("clientNavigation.primary")}>
          {primary.map(button)}
        </nav>
        <details className="client-advanced" key={activeApp}>
          <summary>{t("clientNavigation.advanced")}</summary>
          <nav aria-label={t("clientNavigation.advanced")}>
            {advancedViewsFor(activeApp).map((target) => (
              <button
                key={target}
                type="button"
                disabled={busy}
                className="client-nav-button"
                aria-current={view === target ? "page" : undefined}
                onClick={() => onNavigate(target)}
              >
                {t(VIEW_TITLE_KEYS[target])}
              </button>
            ))}
          </nav>
        </details>
        <nav
          className="client-secondary"
          aria-label={t("clientNavigation.preferences")}
        >
          {secondary.map(button)}
        </nav>
      </aside>
      {children}
    </div>
  );
}
