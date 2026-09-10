import type { CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { AnimatePresence, motion } from "framer-motion";
import {
  Plus,
  Settings,
  ArrowLeft,
  UsersRound,
  BarChart2,
  Download,
  Loader2,
  RefreshCw,
  History,
  FolderArchive,
  Search,
  Wrench,
  Brain,
  LayoutDashboard,
  FolderOpen,
  KeyRound,
  Shield,
  Cpu,
  Book,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { AppSwitcher } from "@/components/AppSwitcher";
import { ProfileSwitcher } from "@/components/profiles/ProfileSwitcher";
import { ProxyToggle } from "@/components/proxy/ProxyToggle";
import { FailoverToggle } from "@/components/proxy/FailoverToggle";
import { ClaudeDesktopRouteToggle } from "@/components/proxy/ClaudeDesktopRouteToggle";
import { RoutingActivationBrand } from "@/components/proxy/RoutingActivationBrand";
import { UpdateBadge } from "@/components/UpdateBadge";
import { McpIcon } from "@/components/BrandIcons";
import {
  getSkillsPageHeaderActions,
  type SkillsPageSource,
  type SkillsPageHandle,
} from "@/components/skills/SkillsPage";
import type { PromptPrimaryAction } from "@/components/prompts/PromptPanel";
import type { SkillsCheckUpdatesState } from "@/components/skills/UnifiedSkillsPanel";
import type { VisibleApps } from "@/types";
import type { AppId } from "@/lib/api/types";
import type { ProxyAppId } from "@/config/appConfig";
import { DRAG_REGION_ATTR, DRAG_REGION_STYLE } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { parentViewFor, VIEW_TITLE_KEYS, type ClientView } from "./navigation";

export interface HeaderRuntime {
  activeApp: AppId;
  sharedFeatureApp: AppId;
  proxyAppId: ProxyAppId | null;
  isProxyRunning: boolean;
  isCurrentAppTakeoverActive: boolean;
  proxyReady: boolean;
  enableLocalProxy: boolean;
  enableFailoverToggle: boolean;
  showProfileSwitcher: boolean;
  visibleApps: VisibleApps;
}
export interface HeaderManagement {
  busy: boolean;
  promptAction: PromptPrimaryAction;
  promptBusy: boolean;
  mcpBusy: boolean;
  skillsBusy: boolean;
  skillsUpdates: SkillsCheckUpdatesState;
  hasUnmanagedSkills: boolean;
  discoverySource: SkillsPageSource;
}
export interface HeaderActions {
  navigate: (view: ClientView) => void;
  selectApp: (app: AppId) => void;
  openSettings: (tab: "general" | "about") => void;
  addProvider: () => void;
  addPrompt: () => void;
  importMcp: () => void;
  addMcp: () => void;
  checkSkills: () => void;
  restoreSkills: () => void;
  installSkillZip: () => void;
  importSkills: () => void;
  discoverSkills: () => void;
  runDiscoveryAction: (
    execute: (page: SkillsPageHandle | null) => void,
  ) => void;
  openHermesWebUI: () => void | Promise<void>;
}
interface Props {
  view: ClientView;
  titlebarHeight: number;
  height: number;
  teamEnabled: boolean;
  runtime: HeaderRuntime;
  management: HeaderManagement;
  actions: HeaderActions;
}
export function ClientHeader({
  view: currentView,
  titlebarHeight: dragBarHeight,
  height,
  teamEnabled,
  runtime,
  management,
  actions,
}: Props) {
  const { t } = useTranslation();
  const {
    activeApp,
    sharedFeatureApp,
    proxyAppId,
    isProxyRunning,
    isCurrentAppTakeoverActive,
    visibleApps,
  } = runtime;
  const {
    busy: managementBusy,
    promptAction: promptPrimaryAction,
    promptBusy: promptManagementBusy,
    mcpBusy: mcpManagementBusy,
    skillsBusy: skillsManagementBusy,
    skillsUpdates: skillsCheckUpdatesState,
    hasUnmanagedSkills,
    discoverySource: skillsDiscoverySource,
  } = management;
  const navigateTo = actions.navigate;
  const hasMcpSupport = sharedFeatureApp !== "pi";
  const hasSkillsSupport = sharedFeatureApp !== "openclaw";
  const hasSessionSupport = [
    "claude",
    "codex",
    "grokbuild",
    "opencode",
    "openclaw",
    "gemini",
    "hermes",
    "pi",
  ].includes(sharedFeatureApp);
  const addActionButtonClass =
    "bg-orange-500 hover:bg-orange-600 dark:bg-orange-500 dark:hover:bg-orange-600 text-white shadow-lg shadow-orange-500/30 dark:shadow-orange-500/40 rounded-full w-8 h-8";
  const titleKey =
    currentView === "settings"
      ? "settings.title"
      : currentView === "mcp"
        ? "mcp.unifiedPanel.title"
        : currentView === "skillsDiscovery"
          ? "skills.title"
          : VIEW_TITLE_KEYS[currentView];
  const pageTitle = t(titleKey, { appName: t(`apps.${sharedFeatureApp}`) });
  return (
    <header
      className="client-page-header fixed z-50 w-full transition-all duration-300 bg-background/80 backdrop-blur-md"
      {...DRAG_REGION_ATTR}
      style={
        {
          ...DRAG_REGION_STYLE,
          top: dragBarHeight,
          height: height,
        } as CSSProperties
      }
    >
      <div
        className="flex h-full items-center justify-between gap-2 px-6"
        {...DRAG_REGION_ATTR}
        style={{ ...DRAG_REGION_STYLE } as CSSProperties}
      >
        <div
          className="flex items-center gap-1"
          style={{ WebkitAppRegion: "no-drag" } as CSSProperties}
        >
          {currentView !== "providers" ? (
            <div className="flex items-center gap-2">
              {currentView !== "home" && (
                <Button
                  variant="outline"
                  size="icon"
                  disabled={managementBusy}
                  aria-label={t("common.back")}
                  onClick={() => navigateTo(parentViewFor(currentView))}
                  className={cn(
                    "mr-2 rounded-lg",
                    managementBusy && "disabled:opacity-100",
                  )}
                >
                  <ArrowLeft className="w-4 h-4" />
                </Button>
              )}
              <h1 className="text-lg font-semibold">{pageTitle}</h1>
            </div>
          ) : (
            <div className="flex items-center gap-2">
              <h1 className="sr-only">{pageTitle}</h1>
              <RoutingActivationBrand
                active={isProxyRunning && isCurrentAppTakeoverActive}
                contextKey={activeApp}
                ready={runtime.proxyReady}
              />
              {teamEnabled && (
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => actions.navigate("team")}
                  title={t("team.title")}
                  className="hover:bg-black/5 dark:hover:bg-white/5"
                >
                  <UsersRound className="h-4 w-4" />
                </Button>
              )}
              <Button
                variant="ghost"
                size="icon"
                onClick={() => {
                  actions.openSettings("general");
                }}
                title={t("common.settings")}
                className="hover:bg-black/5 dark:hover:bg-white/5"
              >
                <Settings className="w-4 h-4" />
              </Button>
              <UpdateBadge
                onClick={() => {
                  actions.openSettings("about");
                }}
              />
              {isCurrentAppTakeoverActive && (
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => {
                    actions.navigate("usage");
                  }}
                  title={t("usage.title", {
                    defaultValue: "使用统计",
                  })}
                  className="hover:bg-black/5 dark:hover:bg-white/5"
                >
                  <BarChart2 className="w-4 h-4" />
                </Button>
              )}
            </div>
          )}
        </div>

        <div className="flex flex-1 min-w-0 items-center justify-end gap-1.5">
          {currentView === "providers" &&
            (activeApp === "claude-desktop" || proxyAppId) && (
              <div
                className="flex shrink-0 items-center gap-1.5"
                style={{ WebkitAppRegion: "no-drag" } as CSSProperties}
              >
                {activeApp === "claude-desktop" ? (
                  <ClaudeDesktopRouteToggle />
                ) : proxyAppId ? (
                  <>
                    {runtime.enableLocalProxy && (
                      <ProxyToggle activeApp={proxyAppId} />
                    )}
                    {runtime.enableFailoverToggle && (
                      <FailoverToggle activeApp={proxyAppId} />
                    )}
                  </>
                ) : null}
              </div>
            )}
          {currentView === "providers" && runtime.showProfileSwitcher && (
            <div
              className="flex shrink-0 items-center"
              style={{ WebkitAppRegion: "no-drag" } as CSSProperties}
            >
              <ProfileSwitcher activeApp={activeApp} />
            </div>
          )}
          {/* 弹性中段：空间不足时由 AppSwitcher 自行收纳溢出应用；
                justify-end + overflow-hidden 只裁剪 resize 瞬间的过渡帧 */}
          <div className="flex flex-1 min-w-0 items-center justify-end overflow-hidden py-4">
            {currentView === "providers" && (
              <AppSwitcher
                activeApp={activeApp}
                onSwitch={actions.selectApp}
                visibleApps={visibleApps}
              />
            )}
          </div>
          {/* 固定右端：主操作（添加供应商等）shrink-0，任何配置下不被挤出 */}
          <div className="flex shrink-0 items-center py-4">
            <div
              className="flex shrink-0 items-center gap-1.5"
              style={{ WebkitAppRegion: "no-drag" } as CSSProperties}
            >
              {currentView === "prompts" && promptPrimaryAction && (
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={promptManagementBusy}
                  onClick={() => actions.addPrompt()}
                  className="hover:bg-black/5 disabled:opacity-100 dark:hover:bg-white/5"
                >
                  <Plus className="w-4 h-4 mr-2" />
                  {t(
                    promptPrimaryAction === "template"
                      ? "pi.prompts.newTemplate"
                      : "prompts.add",
                  )}
                </Button>
              )}
              {currentView === "mcp" && (
                <>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={mcpManagementBusy}
                    onClick={() => actions.importMcp()}
                    className="hover:bg-black/5 disabled:opacity-100 dark:hover:bg-white/5"
                  >
                    <Download className="w-4 h-4 mr-2" />
                    {t("mcp.importExisting")}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={mcpManagementBusy}
                    onClick={() => actions.addMcp()}
                    className="hover:bg-black/5 disabled:opacity-100 dark:hover:bg-white/5"
                  >
                    <Plus className="w-4 h-4 mr-2" />
                    {t("mcp.addMcp")}
                  </Button>
                </>
              )}
              {currentView === "skills" && (
                <>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={
                      skillsManagementBusy ||
                      skillsCheckUpdatesState.isChecking ||
                      !skillsCheckUpdatesState.hasSkills
                    }
                    onClick={() => actions.checkSkills()}
                    className={cn(
                      "hover:bg-black/5 dark:hover:bg-white/5",
                      skillsManagementBusy && "disabled:opacity-100",
                    )}
                  >
                    {skillsCheckUpdatesState.isChecking ? (
                      <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                    ) : (
                      <RefreshCw className="w-4 h-4 mr-2" />
                    )}
                    {skillsCheckUpdatesState.isChecking
                      ? t("skills.checkingUpdates")
                      : t("skills.checkUpdates")}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={skillsManagementBusy}
                    onClick={() => actions.restoreSkills()}
                    className="hover:bg-black/5 disabled:opacity-100 dark:hover:bg-white/5"
                  >
                    <History className="w-4 h-4 mr-2" />
                    {t("skills.restoreFromBackup.button")}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={skillsManagementBusy}
                    onClick={() => actions.installSkillZip()}
                    className="hover:bg-black/5 disabled:opacity-100 dark:hover:bg-white/5"
                  >
                    <FolderArchive className="w-4 h-4 mr-2" />
                    {t("skills.installFromZip.button")}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={skillsManagementBusy}
                    onClick={() => actions.importSkills()}
                    className="relative hover:bg-black/5 disabled:opacity-100 dark:hover:bg-white/5"
                    title={
                      hasUnmanagedSkills
                        ? t("skills.unmanagedAvailable")
                        : undefined
                    }
                  >
                    <Download className="w-4 h-4 mr-2" />
                    {t("skills.import")}
                    {hasUnmanagedSkills && (
                      <span
                        className="absolute top-1 right-1 h-2 w-2 rounded-full bg-green-500"
                        aria-hidden="true"
                      />
                    )}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={skillsManagementBusy}
                    onClick={() => actions.discoverSkills()}
                    className="hover:bg-black/5 disabled:opacity-100 dark:hover:bg-white/5"
                  >
                    <Search className="w-4 h-4 mr-2" />
                    {t("skills.discover")}
                  </Button>
                </>
              )}
              {currentView === "skillsDiscovery" && (
                <>
                  {getSkillsPageHeaderActions(skillsDiscoverySource).map(
                    ({ key, labelKey, Icon, execute }) => (
                      <Button
                        key={key}
                        variant="ghost"
                        size="sm"
                        onClick={() => actions.runDiscoveryAction(execute)}
                        className="hover:bg-black/5 dark:hover:bg-white/5"
                      >
                        <Icon className="w-4 h-4 mr-2" />
                        {t(labelKey)}
                      </Button>
                    ),
                  )}
                </>
              )}
              {currentView === "providers" && (
                <>
                  <div className="flex items-center gap-1 p-1 bg-muted rounded-xl">
                    <AnimatePresence mode="wait">
                      <motion.div
                        key={
                          activeApp === "openclaw"
                            ? "openclaw"
                            : activeApp === "hermes"
                              ? "hermes"
                              : activeApp === "grokbuild"
                                ? "grokbuild"
                                : "default"
                        }
                        className="flex items-center gap-1"
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        transition={{ duration: 0.15 }}
                      >
                        {activeApp === "hermes" ? (
                          <>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => actions.navigate("skills")}
                              className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5 w-8 px-2"
                              title={t("skills.manage")}
                            >
                              <Wrench className="w-4 h-4" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => actions.navigate("hermesMemory")}
                              className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5 w-8 px-2"
                              title={t("hermes.memory.title")}
                            >
                              <Brain className="w-4 h-4" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => void actions.openHermesWebUI()}
                              className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5 w-8 px-2"
                              title={t("hermes.webui.open")}
                            >
                              <LayoutDashboard className="w-4 h-4" />
                            </Button>
                            {hasMcpSupport && (
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => actions.navigate("mcp")}
                                className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5 w-8 px-2"
                                title={t("mcp.title")}
                              >
                                <McpIcon size={16} />
                              </Button>
                            )}
                          </>
                        ) : activeApp === "openclaw" ? (
                          <>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => actions.navigate("workspace")}
                              className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5 w-8 px-2"
                              title={t("workspace.manage")}
                            >
                              <FolderOpen className="w-4 h-4" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => actions.navigate("openclawEnv")}
                              className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5 w-8 px-2"
                              title={t("openclaw.env.title")}
                            >
                              <KeyRound className="w-4 h-4" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => actions.navigate("openclawTools")}
                              className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5 w-8 px-2"
                              title={t("openclaw.tools.title")}
                            >
                              <Shield className="w-4 h-4" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => actions.navigate("openclawAgents")}
                              className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5 w-8 px-2"
                              title={t("openclaw.agents.title")}
                            >
                              <Cpu className="w-4 h-4" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => actions.navigate("sessions")}
                              className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5 w-8 px-2"
                              title={t("sessionManager.title")}
                            >
                              <History className="w-4 h-4" />
                            </Button>
                          </>
                        ) : (
                          <>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => actions.navigate("skills")}
                              className={cn(
                                "text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5",
                                "transition-all duration-200 ease-in-out overflow-hidden",
                                hasSkillsSupport
                                  ? "opacity-100 w-8 scale-100 px-2"
                                  : "opacity-0 w-0 scale-75 pointer-events-none px-0 -ml-1",
                              )}
                              title={t("skills.manage")}
                            >
                              <Wrench className="flex-shrink-0 w-4 h-4" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => actions.navigate("prompts")}
                              className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5 w-8 px-2"
                              title={t("prompts.manage")}
                            >
                              <Book className="w-4 h-4" />
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => actions.navigate("sessions")}
                              className={cn(
                                "text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5",
                                "transition-all duration-200 ease-in-out overflow-hidden",
                                hasSessionSupport
                                  ? "opacity-100 w-8 scale-100 px-2"
                                  : "opacity-0 w-0 scale-75 pointer-events-none px-0 -ml-1",
                              )}
                              title={t("sessionManager.title")}
                            >
                              <History className="flex-shrink-0 w-4 h-4" />
                            </Button>
                            {hasMcpSupport && (
                              <Button
                                variant="ghost"
                                size="sm"
                                onClick={() => actions.navigate("mcp")}
                                className="text-muted-foreground hover:text-foreground hover:bg-black/5 dark:hover:bg-white/5 w-8 px-2"
                                title={t("mcp.title")}
                              >
                                <McpIcon size={16} />
                              </Button>
                            )}
                          </>
                        )}
                      </motion.div>
                    </AnimatePresence>
                  </div>

                  <Button
                    onClick={() => actions.addProvider()}
                    size="icon"
                    className={`ml-2 ${addActionButtonClass}`}
                    aria-label={t("provider.addNewProvider")}
                    title={t("provider.addNewProvider")}
                  >
                    <Plus className="w-5 h-5" />
                  </Button>
                </>
              )}
            </div>
          </div>
        </div>
      </div>
    </header>
  );
}
