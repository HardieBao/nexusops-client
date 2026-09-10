import type { AppId } from "@/lib/api/types";

export const CLIENT_VIEWS = [
  "home",
  "providers",
  "assetLibrary",
  "usage",
  "settings",
  "prompts",
  "skills",
  "skillsDiscovery",
  "mcp",
  "agents",
  "universal",
  "sessions",
  "workspace",
  "openclawEnv",
  "openclawTools",
  "openclawAgents",
  "hermesMemory",
  "team",
  "teamProviders",
  "teamAssets",
] as const;
export type ClientView = (typeof CLIENT_VIEWS)[number];
export type MainView =
  | "home"
  | "providers"
  | "assetLibrary"
  | "usage"
  | "team"
  | "settings";
export const VIEW_STORAGE_KEY = "nexusops-client-last-view";

export function restoreClientView(value: string | null): ClientView {
  return CLIENT_VIEWS.includes(value as ClientView)
    ? (value as ClientView)
    : "home";
}

export function mainViewFor(view: ClientView): MainView {
  if (view === "teamProviders") return "providers";
  if (view === "teamAssets") return "assetLibrary";
  if (["skills", "skillsDiscovery", "prompts", "agents"].includes(view))
    return "assetLibrary";
  if (view === "sessions") return "usage";
  if (
    [
      "mcp",
      "universal",
      "workspace",
      "openclawEnv",
      "openclawTools",
      "openclawAgents",
      "hermesMemory",
    ].includes(view)
  )
    return "providers";
  return view as MainView;
}

export function parentViewFor(view: ClientView): ClientView {
  if (view === "skillsDiscovery") return "skills";
  const main = mainViewFor(view);
  return main === view ? "home" : main;
}

export function advancedViewsFor(app: AppId): ClientView[] {
  const shared = app === "claude-desktop" ? "claude" : app;
  const views: ClientView[] = ["universal", "sessions"];
  if (shared !== "pi" && shared !== "openclaw") views.push("mcp");
  if (shared === "openclaw")
    views.push("workspace", "openclawEnv", "openclawTools", "openclawAgents");
  if (shared === "hermes") views.push("hermesMemory");
  return views;
}

export const VIEW_TITLE_KEYS: Record<ClientView, string> = {
  home: "clientNavigation.home",
  providers: "clientNavigation.providers",
  assetLibrary: "clientNavigation.assetLibrary",
  usage: "clientNavigation.usage",
  team: "clientNavigation.team",
  teamProviders: "team.provider.title",
  teamAssets: "clientNavigation.teamAssets",
  settings: "common.settings",
  prompts: "prompts.title",
  skills: "skills.title",
  skillsDiscovery: "clientNavigation.skillsDiscovery",
  mcp: "mcp.title",
  agents: "agents.title",
  universal: "universalProvider.title",
  sessions: "sessionManager.title",
  workspace: "workspace.title",
  openclawEnv: "openclaw.env.title",
  openclawTools: "openclaw.tools.title",
  openclawAgents: "openclaw.agents.title",
  hermesMemory: "hermes.memory.title",
};
