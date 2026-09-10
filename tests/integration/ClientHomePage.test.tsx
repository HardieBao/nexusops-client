import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, fireEvent } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ClientHomePage } from "@/features/shell/ClientHomePage";
import {
  getTeamStatus,
  getToolObservation,
  type TeamConnection,
} from "@/features/team/api";

vi.mock("@/features/team/api", () => ({
  getTeamStatus: vi.fn(),
  getToolObservation: vi.fn(),
}));
beforeEach(() => vi.mocked(getTeamStatus).mockReset().mockResolvedValue(null));
function home(teamEnabled: boolean) {
  const navigate = vi.fn();
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <ClientHomePage
        teamEnabled={teamEnabled}
        activeApp="codex"
        providerName="Personal provider"
        onNavigate={navigate}
      />
    </QueryClientProvider>,
  );
  return navigate;
}
describe("workspace facts and first-run state", () => {
  it("shows independent tool facts and navigates to recovery without mutations", async () => {
    vi.mocked(getTeamStatus).mockResolvedValue({
      id: "workspace",
      profile: { name: "Team", key: { id: 7 } },
      status: "connected",
      last_checked_at: "2026-09-11T00:00:00Z",
    } as TeamConnection);
    vi.mocked(getToolObservation).mockResolvedValue([
      {
        runtime: "codex",
        registration: "needs_repair",
        collection_enabled: true,
        observation: "stale",
        last_observed_at: "2026-09-09T00:00:00Z",
      },
    ]);
    const navigate = home(true);
    expect(
      await screen.findByText("team.usageHistory.registration.needs_repair"),
    ).toBeInTheDocument();
    expect(
      screen.getByText("team.usageHistory.observation.stale"),
    ).toBeInTheDocument();
    expect(getToolObservation).toHaveBeenCalledTimes(1);
    fireEvent.click(
      screen.getByRole("button", { name: "clientNavigation.usage" }),
    );
    expect(navigate).toHaveBeenCalledWith("usage");
  });
  it("shows first-run guidance and does not connect, switch, or register anything", async () => {
    const navigate = home(true);
    expect(
      await screen.findByText("clientHome.connectHint"),
    ).toBeInTheDocument();
    expect(getTeamStatus).toHaveBeenCalledTimes(1);
    expect(navigate).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "clientHome.connect" }));
    expect(navigate).toHaveBeenCalledWith("team");
  });
  it("does not call Team IPC when the build disables Team", () => {
    home(false);
    expect(getTeamStatus).not.toHaveBeenCalled();
    expect(screen.getByText("Personal provider")).toBeInTheDocument();
    expect(
      screen.getByText("clientHome.configurationOnly"),
    ).toBeInTheDocument();
  });
});
