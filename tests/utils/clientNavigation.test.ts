import { describe, expect, it } from "vitest";
import {
  CLIENT_VIEWS,
  restoreClientView,
  mainViewFor,
  parentViewFor,
  advancedViewsFor,
} from "@/features/shell/navigation";

describe("client view persistence and legacy entry mapping", () => {
  it.each(CLIENT_VIEWS)(
    "restores the saved %s entry without dropping its feature",
    (view) => {
      expect(restoreClientView(view)).toBe(view);
    },
  );
  it.each([null, "", "unknown", "__proto__"])(
    "uses a real workspace fallback for %s",
    (value) => {
      expect(restoreClientView(value)).toBe("home");
    },
  );
  it("keeps the local workspace distinct from the new home and returns discovery to skills", () => {
    expect(restoreClientView("workspace")).toBe("workspace");
    expect(mainViewFor("workspace")).toBe("providers");
    expect(mainViewFor("sessions")).toBe("usage");
    expect(mainViewFor("prompts")).toBe("assetLibrary");
    expect(parentViewFor("skillsDiscovery")).toBe("skills");
  });
  it("preserves native runtime-specific tools without manufacturing Pi MCP support", () => {
    expect(advancedViewsFor("openclaw")).toEqual(
      expect.arrayContaining([
        "workspace",
        "openclawEnv",
        "openclawTools",
        "openclawAgents",
      ]),
    );
    expect(advancedViewsFor("hermes")).toContain("hermesMemory");
    expect(advancedViewsFor("pi")).not.toContain("mcp");
    expect(advancedViewsFor("claude-desktop")).toContain("mcp");
  });
});
