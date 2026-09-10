import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToolUsageHistoryPanel } from "@/features/team/ToolUsageHistoryPanel";
import {
  getToolUsageHistory,
  clearToolUsageHistory,
  getToolObservation,
} from "@/features/team/api";

const workspace = vi.hoisted(() => ({
  connection: { id: "workspace", profile: { key: { id: 1 } } },
  busy: null,
  setReportingBusy: vi.fn(),
}));
vi.mock("@/features/team/TeamWorkspaceBoundary", () => ({
  useSharedTeamWorkspace: () => workspace,
}));
vi.mock("@/features/team/api", () => ({
  getToolUsageHistory: vi.fn(),
  clearToolUsageHistory: vi.fn(),
  getToolObservation: vi.fn(),
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
const days = [
  {
    day: "2026-09-11",
    runtime: "codex",
    event: "turn.completed",
    count: 20,
    last_observed_at: "2026-09-11T09:00:00+00:00",
  },
];
beforeEach(() => {
  vi.resetAllMocks();
  workspace.connection.profile.key.id = 1;
  vi.mocked(getToolUsageHistory).mockResolvedValue(days);
  vi.mocked(getToolObservation).mockResolvedValue([
    {
      runtime: "codex",
      registration: "needs_repair",
      collection_enabled: true,
      observation: "stale",
      last_observed_at: days[0].last_observed_at,
    },
  ]);
  vi.mocked(clearToolUsageHistory).mockResolvedValue();
});

describe("local tool history", () => {
  it("shows real counts and requires an explicit clear confirmation", async () => {
    render(<ToolUsageHistoryPanel />);
    expect(await screen.findByText("20")).toBeInTheDocument();
    expect(
      screen.getByText("team.usageHistory.registration.needs_repair"),
    ).toBeInTheDocument();
    expect(
      screen.getByText("team.usageHistory.collection.enabled"),
    ).toBeInTheDocument();
    expect(
      screen.getByText("team.usageHistory.observation.stale"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByText("team.usageHistory.clear"));
    expect(clearToolUsageHistory).not.toHaveBeenCalled();
    vi.mocked(getToolUsageHistory).mockResolvedValue([]);
    fireEvent.click(screen.getByText("team.usageHistory.confirmClear"));
    await waitFor(() => expect(clearToolUsageHistory).toHaveBeenCalledTimes(1));
    expect(
      await screen.findByText("team.usageHistory.empty"),
    ).toBeInTheDocument();
    expect(workspace.setReportingBusy).toHaveBeenCalledWith(true);
    expect(workspace.setReportingBusy).toHaveBeenLastCalledWith(false);
  });

  it("does not display a late response from the previous identity", async () => {
    let finish!: (value: typeof days) => void;
    vi.mocked(getToolUsageHistory).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const view = render(<ToolUsageHistoryPanel />);
    workspace.connection.profile.key.id = 2;
    vi.mocked(getToolUsageHistory).mockResolvedValue([]);
    view.rerender(<ToolUsageHistoryPanel />);
    expect(
      await screen.findByText("team.usageHistory.empty"),
    ).toBeInTheDocument();
    finish(days);
    await waitFor(() =>
      expect(screen.queryByText("20")).not.toBeInTheDocument(),
    );
  });

  it("distinguishes failed reads from empty history and supports tool filtering", async () => {
    vi.mocked(getToolUsageHistory).mockRejectedValueOnce(
      new Error("unavailable"),
    );
    render(<ToolUsageHistoryPanel />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "team.usageHistory.error",
    );
    expect(
      screen.queryByText("team.usageHistory.empty"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("common.refresh"));
    expect(await screen.findByText("20")).toBeInTheDocument();
    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "claude-code" },
    });
    expect(screen.getByText("team.usageHistory.empty")).toBeInTheDocument();
  });
});
