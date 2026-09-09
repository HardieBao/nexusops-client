import { render, screen, within, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ClientShell } from "@/features/shell/ClientShell";

describe("persistent client navigation", () => {
  it("keeps usage reachable and groups the existing asset editors", () => {
    const navigate = vi.fn();
    render(
      <ClientShell
        view="prompts"
        activeApp="codex"
        teamEnabled
        titlebarHeight={0}
        busy={false}
        onNavigate={navigate}
      >
        <div>Current editor</div>
      </ClientShell>,
    );
    const primary = within(
      screen.getByRole("navigation", { name: "clientNavigation.primary" }),
    );
    expect(
      primary.getByRole("button", { name: "clientNavigation.assetLibrary" }),
    ).toHaveAttribute("aria-current", "page");
    fireEvent.click(
      primary.getByRole("button", { name: "clientNavigation.usage" }),
    );
    expect(navigate).toHaveBeenCalledWith("usage");
    expect(screen.getByText("Current editor")).toBeInTheDocument();
  });
  it("blocks navigation while an existing editor owns an unfinished operation", () => {
    const navigate = vi.fn();
    render(
      <ClientShell
        view="skills"
        activeApp="codex"
        teamEnabled
        titlebarHeight={32}
        busy
        onNavigate={navigate}
      >
        <div>Editing</div>
      </ClientShell>,
    );
    for (const button of screen.getAllByRole("button")) {
      expect(button).toBeDisabled();
      fireEvent.click(button);
    }
    expect(navigate).not.toHaveBeenCalled();
  });
  it("keeps local navigation usable in a build without Team", () => {
    const navigate = vi.fn();
    render(
      <ClientShell
        view="home"
        activeApp="pi"
        teamEnabled={false}
        titlebarHeight={0}
        busy={false}
        onNavigate={navigate}
      >
        <div>Local</div>
      </ClientShell>,
    );
    expect(
      screen.getByRole("button", { name: "clientNavigation.team" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "clientNavigation.usage" }),
    ).toBeEnabled();
    expect(
      screen.getByRole("button", { name: "clientNavigation.providers" }),
    ).toBeEnabled();
  });
});
