import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  ClientHeader,
  type HeaderActions,
  type HeaderManagement,
  type HeaderRuntime,
} from "@/features/shell/ClientHeader";
import { DEFAULT_VISIBLE_APPS } from "@/config/appConfig";

vi.mock("@/components/UpdateBadge", () => ({ UpdateBadge: () => null }));
vi.mock("@/components/AppSwitcher", () => ({
  AppSwitcher: () => <div>App switcher</div>,
}));
vi.mock("@/components/profiles/ProfileSwitcher", () => ({
  ProfileSwitcher: () => null,
}));
vi.mock("@/components/proxy/RoutingActivationBrand", () => ({
  RoutingActivationBrand: () => <span>NexusOps</span>,
}));
const runtime: HeaderRuntime = {
  activeApp: "codex",
  sharedFeatureApp: "codex",
  proxyAppId: "codex",
  isProxyRunning: false,
  isCurrentAppTakeoverActive: false,
  proxyReady: true,
  enableLocalProxy: false,
  enableFailoverToggle: false,
  showProfileSwitcher: false,
  visibleApps: DEFAULT_VISIBLE_APPS,
};
const management: HeaderManagement = {
  busy: false,
  promptAction: "prompt",
  promptBusy: false,
  mcpBusy: false,
  skillsBusy: false,
  skillsUpdates: { isChecking: false, hasSkills: true },
  hasUnmanagedSkills: false,
  discoverySource: "repos",
};
function actions(): HeaderActions {
  return {
    navigate: vi.fn(),
    selectApp: vi.fn(),
    openSettings: vi.fn(),
    addProvider: vi.fn(),
    addPrompt: vi.fn(),
    importMcp: vi.fn(),
    addMcp: vi.fn(),
    checkSkills: vi.fn(),
    restoreSkills: vi.fn(),
    installSkillZip: vi.fn(),
    importSkills: vi.fn(),
    discoverSkills: vi.fn(),
    runDiscoveryAction: vi.fn(),
    openHermesWebUI: vi.fn(),
  };
}
describe("client page header contract", () => {
  it("shows a first-class usage title and returns to the workspace", () => {
    const calls = actions();
    render(
      <ClientHeader
        view="usage"
        titlebarHeight={32}
        height={64}
        teamEnabled
        runtime={runtime}
        management={management}
        actions={calls}
      />,
    );
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      "clientNavigation.usage",
    );
    fireEvent.click(screen.getByRole("button", { name: "common.back" }));
    expect(calls.navigate).toHaveBeenCalledWith("home");
  });
  it("does not offer a dead back button on home", () => {
    render(
      <ClientHeader
        view="home"
        titlebarHeight={0}
        height={64}
        teamEnabled
        runtime={runtime}
        management={management}
        actions={actions()}
      />,
    );
    expect(
      screen.queryByRole("button", { name: "common.back" }),
    ).not.toBeInTheDocument();
  });
  it("preserves MCP operation guards", () => {
    const calls = actions();
    render(
      <ClientHeader
        view="mcp"
        titlebarHeight={0}
        height={64}
        teamEnabled
        runtime={runtime}
        management={{ ...management, busy: true, mcpBusy: true }}
        actions={calls}
      />,
    );
    for (const label of ["common.back", "mcp.importExisting", "mcp.addMcp"]) {
      const button = screen.getByRole("button", { name: label });
      expect(button).toBeDisabled();
      fireEvent.click(button);
    }
    expect(calls.importMcp).not.toHaveBeenCalled();
    expect(calls.addMcp).not.toHaveBeenCalled();
    expect(calls.navigate).not.toHaveBeenCalled();
  });
  it("preserves the template primary action and its callback", () => {
    const calls = actions();
    render(
      <ClientHeader
        view="prompts"
        titlebarHeight={0}
        height={64}
        teamEnabled
        runtime={runtime}
        management={{ ...management, promptAction: "template" }}
        actions={calls}
      />,
    );
    fireEvent.click(
      screen.getByRole("button", { name: "pi.prompts.newTemplate" }),
    );
    expect(calls.addPrompt).toHaveBeenCalledTimes(1);
  });
  it("keeps skill update, backup, import and discovery actions separate", () => {
    const calls = actions();
    render(
      <ClientHeader
        view="skills"
        titlebarHeight={0}
        height={64}
        teamEnabled
        runtime={runtime}
        management={{
          ...management,
          skillsUpdates: { isChecking: true, hasSkills: true },
        }}
        actions={calls}
      />,
    );
    expect(
      screen.getByRole("button", { name: "skills.checkingUpdates" }),
    ).toBeDisabled();
    fireEvent.click(
      screen.getByRole("button", { name: "skills.restoreFromBackup.button" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "skills.import" }));
    fireEvent.click(screen.getByRole("button", { name: "skills.discover" }));
    expect(calls.checkSkills).not.toHaveBeenCalled();
    expect(calls.restoreSkills).toHaveBeenCalledTimes(1);
    expect(calls.importSkills).toHaveBeenCalledTimes(1);
    expect(calls.discoverSkills).toHaveBeenCalledTimes(1);
  });
});
