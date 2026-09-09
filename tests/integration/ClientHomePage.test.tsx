import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, fireEvent } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ClientHomePage } from "@/features/shell/ClientHomePage";
import { getTeamStatus } from "@/features/team/api";

vi.mock("@/features/team/api", () => ({ getTeamStatus: vi.fn() }));
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
