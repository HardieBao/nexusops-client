import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ToolUsagePanel } from "@/features/team/ToolUsagePanel";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("react-i18next", async (importOriginal) => ({
  ...(await importOriginal<typeof import("react-i18next")>()),
  useTranslation: () => ({ i18n: { language: "en" } }),
}));
const disabled = {
  enabled: false,
  pending: 0,
  uploaded: 0,
  last_uploaded_at: null,
  last_error: null,
};
beforeEach(() => {
  vi.mocked(invoke).mockReset().mockResolvedValue(disabled);
});
describe("embedded SkillOps collection controls", () => {
  it("does not install or upload until the employee explicitly enables collection", async () => {
    render(<ToolUsagePanel />);
    await screen.findByText(/Collection disabled/);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(
      screen.getByText(/organization administrators can see/),
    ).toBeInTheDocument();
    vi.mocked(invoke).mockResolvedValueOnce({
      ...disabled,
      enabled: true,
      pending: 2,
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Enable collection and upload" }),
    );
    await screen.findByRole("button", { name: "Upload now" });
    expect(invoke).toHaveBeenLastCalledWith("team_configure_tool_usage", {
      enabled: true,
    });
    vi.mocked(invoke).mockResolvedValueOnce(disabled);
    fireEvent.click(
      screen.getByRole("button", { name: "Disable and clear pending" }),
    );
    await screen.findByText(/Collection disabled/);
    expect(invoke).toHaveBeenLastCalledWith("team_configure_tool_usage", {
      enabled: false,
    });
  });
  it("shows failed uploads without displaying raw errors or declaring success", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      ...disabled,
      enabled: true,
      pending: 3,
    });
    render(<ToolUsagePanel />);
    await screen.findByRole("button", { name: "Upload now" });
    vi.mocked(invoke).mockRejectedValueOnce(new Error("PRIVATE_KEY_AND_PATH"));
    fireEvent.click(screen.getByRole("button", { name: "Upload now" }));
    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    expect(screen.queryByText(/PRIVATE_KEY/)).not.toBeInTheDocument();
    expect(screen.getByText(/Pending 3/)).toBeInTheDocument();
  });
});
