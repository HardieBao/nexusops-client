import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TeamPage, type TeamSection } from "@/features/team/TeamPage";
import { TeamWorkspaceBoundary } from "@/features/team/TeamWorkspaceBoundary";
import type { TeamConnection, SyncPlan } from "@/features/team/api";

const api = vi.hoisted(() => ({
  getTeamStatus: vi.fn(),
  previewTeamSync: vi.fn(),
  getTeamLocalHistory: vi.fn(),
  cancelTeamOperation: vi.fn(),
  connectTeam: vi.fn(),
  disconnectTeam: vi.fn(),
  previewTeamProvider: vi.fn(),
  applyTeamProvider: vi.fn(),
  activateTeamProvider: vi.fn(),
  restoreTeamBackup: vi.fn(),
  syncTeam: vi.fn(),
}));
vi.mock("@/features/team/api", () => api);
vi.mock("@/features/team/ToolUsagePanel", () => ({
  ToolUsagePanel: () => <div>Reporting panel</div>,
}));
const connection: TeamConnection = {
  id: "synthetic-connection",
  gateway_url: "https://gateway.example/",
  status: "connected",
  last_checked_at: "2026-09-10T00:00:00Z",
  last_error: null,
  profile: {
    schema_version: 1,
    organization_id: "fixture-org",
    workspace_id: "fixture-workspace",
    name: "Fixture organization",
    gateway_url: "https://gateway.example/",
    base_url: "https://gateway.example/v1",
    key: {
      id: 7,
      name: "Fixture key",
      prefix: "nx_",
      last_four: "test",
      platform: "openai",
      allow_messages_dispatch: false,
    },
    models: [],
    asset_manifest_url: "/api/v1/me/assets/manifest",
    subscriptions: [],
    limits: {
      max_assets: 100,
      max_revisions_per_asset: 100,
      max_text_bytes: 1048576,
      max_archive_bytes: 20971520,
      max_unpacked_bytes: 104857600,
      max_files: 2000,
      max_path_depth: 16,
      max_path_bytes: 240,
    },
  },
};
const plan: SyncPlan = {
  connection,
  items: [],
  conflicts: [],
  withdrawn: [],
  last_successful_sync: null,
};
beforeEach(() => {
  for (const mock of Object.values(api)) mock.mockReset();
  api.getTeamStatus.mockResolvedValue(connection);
  api.previewTeamSync.mockResolvedValue(plan);
  api.getTeamLocalHistory.mockResolvedValue([]);
  api.cancelTeamOperation.mockResolvedValue(undefined);
  api.connectTeam.mockResolvedValue(connection);
});

describe("shared Team operation ownership", () => {
  it("keeps one controller across organization, provider and asset pages", async () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const busy = vi.fn();
    const element = (section: TeamSection) => (
      <QueryClientProvider client={queryClient}>
        <TeamWorkspaceBoundary active initialApp="claude" onBusyChange={busy}>
          <TeamPage section={section} />
        </TeamWorkspaceBoundary>
      </QueryClientProvider>
    );
    const view = render(element("organization"));
    await screen.findByText(connection.profile.name);
    await waitFor(() =>
      expect(api.previewTeamSync).toHaveBeenCalledWith("claude"),
    );
    expect(screen.queryByText("team.provider.title")).not.toBeInTheDocument();
    view.rerender(element("providers"));
    await screen.findByText("team.provider.title");
    expect(screen.queryByText("Reporting panel")).not.toBeInTheDocument();
    view.rerender(element("assets"));
    await screen.findByText("team.assets.title");
    expect(api.getTeamStatus).toHaveBeenCalledTimes(1);
    expect(api.previewTeamSync).toHaveBeenCalledTimes(1);
    expect(api.connectTeam).not.toHaveBeenCalled();
    expect(api.cancelTeamOperation).not.toHaveBeenCalled();
    view.unmount();
    expect(api.cancelTeamOperation).toHaveBeenCalledTimes(1);
  });
  it("does not mount Team commands when the build feature is disabled", () => {
    render(
      <TeamWorkspaceBoundary
        active={false}
        initialApp="codex"
        onBusyChange={vi.fn()}
      >
        <div>Local page</div>
      </TeamWorkspaceBoundary>,
    );
    expect(screen.getByText("Local page")).toBeInTheDocument();
    expect(api.getTeamStatus).not.toHaveBeenCalled();
  });
  it("reports an in-flight refresh to the shell and releases the guard on completion", async () => {
    const queryClient = new QueryClient();
    const busy = vi.fn();
    render(
      <QueryClientProvider client={queryClient}>
        <TeamWorkspaceBoundary active initialApp="codex" onBusyChange={busy}>
          <TeamPage section="organization" />
        </TeamWorkspaceBoundary>
      </QueryClientProvider>,
    );
    await screen.findByText(connection.profile.name);
    await waitFor(() => expect(api.previewTeamSync).toHaveBeenCalledTimes(1));
    let finish!: (value: SyncPlan) => void;
    api.previewTeamSync.mockImplementationOnce(
      () =>
        new Promise<SyncPlan>((resolve) => {
          finish = resolve;
        }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "team.actions.refresh" }),
    );
    await waitFor(() => expect(busy).toHaveBeenLastCalledWith(true));
    await act(async () => finish(plan));
    await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
  });
  it("ignores a delayed saved-status response after a newer explicit connection", async () => {
    let oldStatus!: (value: TeamConnection) => void;
    api.getTeamStatus.mockImplementationOnce(
      () =>
        new Promise<TeamConnection>((resolve) => {
          oldStatus = resolve;
        }),
    );
    render(<TeamPage section="organization" />);
    fireEvent.change(screen.getByLabelText("team.connect.gateway"), {
      target: { value: connection.gateway_url },
    });
    fireEvent.change(screen.getByLabelText("team.connect.key"), {
      target: { value: "nx_synthetic_only" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "team.connect.submit" }),
    );
    await screen.findByText(connection.profile.name);
    await act(async () =>
      oldStatus({ ...connection, status: "authentication_required" }),
    );
    expect(api.previewTeamSync).toHaveBeenCalledTimes(1);
    expect(
      screen.queryByText("team.status.authentication_required"),
    ).not.toBeInTheDocument();
  });
});
