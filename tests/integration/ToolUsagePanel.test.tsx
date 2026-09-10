import {
  render,
  screen,
  fireEvent,
  waitFor,
  act,
} from "@testing-library/react";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
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
afterEach(() => vi.useRealTimers());
describe("embedded SkillOps collection controls", () => {
  it("explains an old gateway without displaying its raw error", async () => {
    render(<ToolUsagePanel />);
    await screen.findByText(/Collection disabled/);
    vi.mocked(invoke).mockRejectedValueOnce({
      code: "upgrade_required",
      message: "PRIVATE_PATH",
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Enable collection and upload" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /Upgrade the gateway/,
    );
    expect(screen.queryByText(/PRIVATE_PATH/)).not.toBeInTheDocument();
  });
  it("does not let an earlier status poll overwrite an enable result", async () => {
    vi.useFakeTimers();
    render(<ToolUsagePanel />);
    await act(async () => {});
    let finish!: (value: typeof disabled) => void;
    vi.mocked(invoke).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(15000);
    });
    vi.mocked(invoke).mockResolvedValueOnce({ ...disabled, enabled: true });
    fireEvent.click(
      screen.getByRole("button", { name: "Enable collection and upload" }),
    );
    await act(async () => {});
    await act(async () => {
      finish(disabled);
    });
    expect(
      screen.getByRole("button", { name: "Upload now" }),
    ).toBeInTheDocument();
  });
  it("does not start a reporting mutation while another Team operation owns the UI", async () => {
    render(<ToolUsagePanel disabled />);
    await screen.findByText(/Collection disabled/);
    const button = screen.getByRole("button", {
      name: "Enable collection and upload",
    });
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenLastCalledWith("team_tool_usage_status");
  });
  it("holds the shared busy guard until the reporting mutation settles", async () => {
    const changed = vi.fn();
    render(<ToolUsagePanel onBusyChange={changed} />);
    await screen.findByText(/Collection disabled/);
    let finish!: (value: typeof disabled) => void;
    vi.mocked(invoke).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Enable collection and upload" }),
    );
    expect(changed).toHaveBeenLastCalledWith(true);
    finish({ ...disabled, enabled: true });
    await waitFor(() => expect(changed).toHaveBeenLastCalledWith(false));
  });
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
