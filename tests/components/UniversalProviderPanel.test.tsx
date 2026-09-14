import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UniversalProviderPanel } from "@/components/universal/UniversalProviderPanel";
import type { UniversalProvider } from "@/types";

const api = vi.hoisted(() => ({
  getAll: vi.fn(),
  upsert: vi.fn(),
  sync: vi.fn(),
}));
vi.mock("@/lib/api", () => ({ universalProvidersApi: api }));
vi.mock("@/hooks/useDarkMode", () => ({ useDarkMode: () => false }));
vi.mock("@/components/JsonEditor", () => ({ default: () => null }));

beforeEach(() => {
  api.getAll.mockResolvedValue({});
  api.upsert.mockResolvedValue(undefined);
  api.sync.mockResolvedValue(undefined);
});

describe("universal provider creation entry", () => {
  it("opens the real creation form from the empty page and saves a provider", async () => {
    const user = userEvent.setup();
    render(<UniversalProviderPanel />);
    await screen.findByText("还没有统一供应商");
    await user.click(screen.getByRole("button", { name: "添加统一供应商" }));
    await user.clear(screen.getByLabelText("名称"));
    await user.type(screen.getByLabelText("名称"), "Test provider");
    await user.type(
      screen.getByLabelText("API 地址"),
      "https://example.invalid",
    );
    await user.type(screen.getByLabelText("API Key"), "synthetic-test-key");
    await user.click(screen.getByRole("button", { name: "添加" }));
    await waitFor(() =>
      expect(api.upsert).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "Test provider",
          baseUrl: "https://example.invalid",
        }),
      ),
    );
    expect(api.sync).toHaveBeenCalledWith(api.upsert.mock.calls[0][0].id);
  });

  it("keeps creation available when the list already contains a provider", async () => {
    const provider: UniversalProvider = {
      id: "existing",
      name: "Existing provider",
      providerType: "newapi",
      baseUrl: "https://example.invalid",
      apiKey: "synthetic-test-key",
      models: {},
      apps: { claude: true, codex: true, gemini: false },
    };
    api.getAll.mockResolvedValue({ existing: provider });
    const user = userEvent.setup();
    render(<UniversalProviderPanel />);
    await screen.findByText("Existing provider");
    await user.click(screen.getByRole("button", { name: "添加统一供应商" }));
    expect(screen.getByLabelText("API 地址")).toHaveValue("");
    expect(screen.getByLabelText("API Key")).toHaveValue("");
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(api.upsert).not.toHaveBeenCalled();
  });
});
