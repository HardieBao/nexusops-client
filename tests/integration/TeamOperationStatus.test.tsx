import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TeamOperationStatus } from "@/features/team/TeamOperationStatus";

describe("Team operation feedback", () => {
  it("offers stop while running and stays disabled until owned calls settle", () => {
    const cancel = vi.fn().mockResolvedValue(undefined),
      refresh = vi.fn();
    const view = render(
      <TeamOperationStatus
        busy="sync"
        operationNotice={null}
        cancelCurrentOperation={cancel}
        runRefresh={refresh}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "teamOperation.stop" }));
    expect(cancel).toHaveBeenCalledTimes(1);
    view.rerender(
      <TeamOperationStatus
        busy="cancelling"
        operationNotice={null}
        cancelCurrentOperation={cancel}
        runRefresh={refresh}
      />,
    );
    expect(
      screen.getByRole("button", { name: "teamOperation.waiting" }),
    ).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent(
      "teamOperation.settling",
    );
  });
  it("exposes a deliberate recovery action without automatically retrying", () => {
    const refresh = vi.fn().mockResolvedValue(undefined);
    render(
      <TeamOperationStatus
        busy={null}
        operationNotice="recoveryFailed"
        cancelCurrentOperation={vi.fn()}
        runRefresh={refresh}
      />,
    );
    expect(refresh).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: "team.actions.refresh" }),
    );
    expect(refresh).toHaveBeenCalledTimes(1);
  });
  it("does not suggest undoing a disconnect already in progress", () => {
    render(
      <TeamOperationStatus
        busy="disconnect"
        operationNotice={null}
        cancelCurrentOperation={vi.fn()}
        runRefresh={vi.fn()}
      />,
    );
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});
