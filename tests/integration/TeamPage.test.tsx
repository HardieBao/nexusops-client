import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { TeamPage } from "@/features/team/TeamPage";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zh from "@/i18n/locales/zh.json";
import zhTW from "@/i18n/locales/zh-TW.json";

const api = vi.hoisted(() => ({
  getTeamStatus: vi.fn(),
  getTeamLocalHistory: vi.fn(),
  connectTeam: vi.fn(),
  refreshTeam: vi.fn(),
  cancelTeamOperation: vi.fn(),
  disconnectTeam: vi.fn(),
  previewTeamProvider: vi.fn(),
  applyTeamProvider: vi.fn(),
  activateTeamProvider: vi.fn(),
  previewTeamSync: vi.fn(),
  syncTeam: vi.fn(),
  restoreTeamBackup: vi.fn(),
}));

vi.mock("@/features/team/api", () => api);

const connection = {
  id: "connection-fixture",
  gateway_url: "https://gateway.example/",
  profile: {
    schema_version: 1,
    organization_id: "organization-fixture",
    workspace_id: "workspace-fixture",
    name: "Engineering",
    gateway_url: "https://gateway.example/",
    base_url: "https://gateway.example/api/v1",
    key: {
      id: 7,
      name: "Developer key",
      prefix: "nx_",
      last_four: "safe",
      platform: "openai",
      allow_messages_dispatch: false,
    },
    models: [],
    asset_manifest_url: "/api/v1/me/assets/manifest",
    subscriptions: [],
    limits: {
      max_assets: 100,
      max_revisions_per_asset: 100,
      max_text_bytes: 1_048_576,
      max_archive_bytes: 20_971_520,
      max_unpacked_bytes: 104_857_600,
      max_files: 2_000,
      max_path_depth: 16,
      max_path_bytes: 240,
    },
  },
  status: "connected" as const,
  last_checked_at: "2026-09-07T00:00:00Z",
  last_error: null,
};

beforeEach(() => {
  api.getTeamStatus.mockResolvedValue(null);
  api.getTeamLocalHistory.mockResolvedValue([]);
  api.connectTeam.mockResolvedValue(connection);
  api.refreshTeam.mockResolvedValue({
    connection,
    manifest: { schema_version: 1, assets: [], conflicts: [] },
  });
  api.cancelTeamOperation.mockResolvedValue(undefined);
  api.previewTeamSync.mockResolvedValue({
    connection,
    conflicts: [],
    items: [],
    withdrawn: [],
    last_successful_sync: null,
  });
  api.syncTeam.mockResolvedValue({
    connection,
    conflicts: [],
    items: [],
    last_successful_sync: null,
  });
  api.restoreTeamBackup.mockResolvedValue({
    asset_id: 31,
    restored: true,
    has_undo_backup: true,
    upstream_pending: false,
    error_message: null,
  });
});

describe("TeamPage", () => {
  it("keeps disconnect and recovery copy inside every Team locale", () => {
    for (const locale of [en, ja, zh, zhTW]) {
      expect(locale.team.disconnect.title).toBeTruthy();
      expect(locale.team.disconnect.scrubProviders).toBeTruthy();
      expect(locale.team.assets.restoreBackup).toBeTruthy();
      expect(locale.team.assets.restoreConfirmDescription).toBeTruthy();
      expect(locale.team.assets.localHistoryHint).toBeTruthy();
      expect(locale.team.errors.upstream_pending).toBeTruthy();
    }
    expect("disconnect" in en.env).toBe(false);
    expect("disconnect" in zh.env).toBe(false);
  });

  it("submits the member key once and clears it from the page", async () => {
    render(<TeamPage />);

    const gateway = await screen.findByLabelText("team.connect.gateway");
    const key = screen.getByLabelText("team.connect.key");
    const secret = "nx_synthetic_member_secret";
    fireEvent.change(gateway, { target: { value: "https://gateway.example" } });
    fireEvent.change(key, { target: { value: secret } });
    fireEvent.click(
      screen.getByRole("button", { name: "team.connect.submit" }),
    );

    await waitFor(() =>
      expect(api.connectTeam).toHaveBeenCalledWith(
        "https://gateway.example",
        secret,
      ),
    );
    await screen.findByText("Engineering");
    expect(screen.queryByDisplayValue(secret)).not.toBeInTheDocument();
    expect(screen.queryByText(secret)).not.toBeInTheDocument();
  });

  it("reads an exported Team Profile without connecting automatically", async () => {
    render(<TeamPage />);
    const secret = "nx_synthetic_exported_secret";
    const file = new File(
      [
        JSON.stringify({
          schema_version: 1,
          name: "Exported team",
          gateway_url: "https://gateway.example",
          base_url: "https://gateway.example/v1",
          member_key: secret,
        }),
      ],
      "nexusops-team-profile.json",
      { type: "application/json" },
    );
    const input = await screen.findByLabelText("team.connect.profileFile");
    fireEvent.change(input, { target: { files: [file] } });

    await waitFor(() =>
      expect(screen.getByLabelText("team.connect.gateway")).toHaveValue(
        "https://gateway.example",
      ),
    );
    expect(screen.getByLabelText("team.connect.key")).toHaveValue(secret);
    expect(api.connectTeam).not.toHaveBeenCalled();
  });

  it("shows only safe key metadata after restoring a connection", async () => {
    api.getTeamStatus.mockResolvedValue(connection);
    render(<TeamPage />);

    await screen.findByText("Developer key ····safe");
    expect(
      screen.queryByText(/nx_synthetic_member_secret/),
    ).not.toBeInTheDocument();
    expect(api.previewTeamSync).toHaveBeenCalledWith("codex");
  });

  it("keeps a long identity and empty catalog keyboard-operable in both themes", async () => {
    const longName =
      "平台研发与安全联合工作区 · 亚太区基础设施与开发者体验团队";
    api.getTeamStatus.mockResolvedValue({
      ...connection,
      profile: { ...connection.profile, name: longName },
    });

    for (const dark of [false, true]) {
      document.documentElement.classList.toggle("dark", dark);
      const user = userEvent.setup();
      const view = render(<TeamPage />);

      await screen.findByText(longName);
      expect(screen.getByText("team.assets.empty")).toBeInTheDocument();
      const refresh = screen.getByRole("button", {
        name: "team.actions.refresh",
      });
      await user.tab();
      expect(refresh).toHaveFocus();

      view.unmount();
    }
    document.documentElement.classList.remove("dark");
  });

  it("restores an offline local backup once and reports refresh separately", async () => {
    api.getTeamStatus.mockResolvedValue(connection);
    api.previewTeamSync.mockRejectedValue({
      code: "unavailable",
      message: "gateway offline",
    });
    api.getTeamLocalHistory.mockResolvedValue([
      {
        asset_id: 31,
        name: "Offline rule",
        kind: "rule",
        revision: 2,
        last_synced_at: "2026-09-07T00:00:00Z",
        has_backup: true,
        local_file_present: true,
        recovery_error: null,
      },
    ]);

    render(<TeamPage />);
    const restore = await screen.findByRole("button", {
      name: "team.assets.restoreBackup",
    });
    fireEvent.click(restore);
    expect(api.restoreTeamBackup).not.toHaveBeenCalled();
    expect(
      await screen.findByText(/team.assets.restoreConfirmTitle/),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "team.assets.restoreConfirm" }),
    );

    await waitFor(() => expect(api.restoreTeamBackup).toHaveBeenCalledTimes(1));
    expect(api.restoreTeamBackup).toHaveBeenCalledWith("codex", 31);
    expect(
      await screen.findByText("team.errorActions.restore_refresh_failed"),
    ).toBeInTheDocument();
    expect(api.restoreTeamBackup).toHaveBeenCalledTimes(1);
  });

  it("requires an item-level decision before overwriting previewed local content", async () => {
    const asset = {
      asset_id: 31,
      kind: "rule" as const,
      slug: "team-rule",
      name: "Team rule",
      revision: 2,
      content_hash: "a".repeat(64),
      archive_sha256: null,
      download_url: "/api/v1/assets/31/revisions/2/download",
      content_type: "text/markdown; charset=utf-8",
      byte_size: 12,
      files: [],
    };
    api.getTeamStatus.mockResolvedValue(connection);
    api.refreshTeam.mockResolvedValue({
      connection,
      manifest: { schema_version: 1, assets: [asset], conflicts: [] },
    });
    api.previewTeamSync.mockResolvedValue({
      connection,
      conflicts: [],
      items: [
        {
          asset,
          drift: "both_modified",
          support: "managed_download",
          install_path: "C:/fixture/rules/asset-31.md",
          previous_revision: 1,
          subscribed: true,
          has_backup: false,
          last_synced_at: "2026-09-07T00:00:00Z",
          decision_token: "decision-31",
          disk_fingerprint: "local-31",
          upstream_fingerprint: null,
          inspection_error_code: null,
          inspection_error_message: null,
        },
      ],
      withdrawn: [],
      last_successful_sync: "2026-09-07T00:00:00Z",
    });
    api.syncTeam.mockResolvedValue({
      connection,
      conflicts: [],
      items: [
        {
          asset_id: 31,
          revision: 2,
          outcome: "downloaded",
          drift: "both_modified",
          install_path: "C:/fixture/rules/asset-31.md",
          backup_path: "C:/fixture/backups/asset-31.md",
          error_code: null,
          error_message: null,
        },
      ],
      last_successful_sync: "2026-09-07T00:01:00Z",
    });

    render(<TeamPage />);
    await screen.findByText("Team rule");
    const syncButton = screen.getByRole("button", { name: "team.assets.sync" });
    expect(syncButton).toBeEnabled();
    const overwrite = await screen.findByRole("checkbox", {
      name: "team.assets.allowOverwrite",
    });
    fireEvent.click(overwrite);
    fireEvent.click(syncButton);

    await waitFor(() =>
      expect(api.syncTeam).toHaveBeenCalledWith("codex", [
        { asset_id: 31, decision_token: "decision-31" },
      ]),
    );
  });

  it("shows every sync outcome, support mode, and a retained withdrawn file", async () => {
    const outcomes = [
      "installed",
      "downloaded",
      "imported_inactive",
      "unchanged",
      "skipped",
      "conflict",
      "failed",
    ] as const;
    const assets = outcomes.map((outcome, index) => ({
      asset_id: index + 1,
      kind: (index === 0 ? "skill" : index === 2 ? "prompt" : "rule") as
        | "skill"
        | "prompt"
        | "rule",
      slug: `asset-${outcome}`,
      name: `Asset ${outcome}`,
      revision: 2,
      content_hash: "a".repeat(64),
      archive_sha256: index === 0 ? "b".repeat(64) : null,
      download_url: `/api/v1/assets/${index + 1}/revisions/2/download`,
      content_type:
        index === 0 ? "application/gzip" : "text/markdown; charset=utf-8",
      byte_size: 12,
      files: [],
    }));
    const planItems = assets.map((asset, index) => ({
      asset,
      drift: "same" as const,
      support: (["tool_skill", "managed_download", "inactive_prompt"] as const)[
        index % 3
      ],
      install_path: `C:/fixture/${asset.slug}`,
      previous_revision: 2,
      subscribed: true,
      has_backup: false,
      last_synced_at: "2026-09-07T00:00:00Z",
      decision_token: `decision-${asset.asset_id}`,
      disk_fingerprint: `disk-${asset.asset_id}`,
      upstream_fingerprint: null,
      inspection_error_code: null,
      inspection_error_message: null,
    }));
    api.getTeamStatus.mockResolvedValue(connection);
    api.previewTeamSync.mockResolvedValue({
      connection,
      conflicts: [],
      items: planItems,
      withdrawn: [
        {
          asset_id: 99,
          name: "Withdrawn but retained",
          kind: "rule",
          revision: 1,
          last_synced_at: "2026-09-07T00:00:00Z",
          has_backup: true,
          local_file_present: true,
          recovery_error: null,
        },
      ],
      last_successful_sync: "2026-09-07T00:00:00Z",
    });
    api.syncTeam.mockResolvedValue({
      connection,
      conflicts: [],
      items: outcomes.map((outcome, index) => ({
        asset_id: index + 1,
        revision: 2,
        outcome,
        drift: "same" as const,
        install_path: `C:/fixture/asset-${outcome}`,
        backup_path: null,
        error_code: outcome === "failed" ? "io" : null,
        error_message: outcome === "failed" ? "synthetic failure" : null,
      })),
      last_successful_sync: "2026-09-07T00:01:00Z",
    });

    render(<TeamPage />);
    await screen.findByText("Withdrawn but retained");
    for (const support of [
      "tool_skill",
      "managed_download",
      "inactive_prompt",
    ]) {
      expect(
        screen.getAllByText(`team.assets.support.${support}`).length,
      ).toBeGreaterThan(0);
    }
    expect(
      screen.getByText("team.assets.withdrawnPresent"),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "team.assets.sync" }));
    for (const outcome of outcomes) {
      expect(
        await screen.findByText(`team.assets.outcome.${outcome}`),
      ).toBeInTheDocument();
    }
  });

  it("keeps retained local history visible for every remote access failure", async () => {
    api.getTeamLocalHistory.mockResolvedValue([
      {
        asset_id: 88,
        name: "Retained local asset",
        kind: "rule",
        revision: 4,
        last_synced_at: "2026-09-07T00:00:00Z",
        has_backup: true,
        local_file_present: true,
        recovery_error: null,
      },
    ]);

    for (const code of [
      "authentication_required",
      "access_denied",
      "conflict",
      "unavailable",
    ]) {
      api.getTeamStatus.mockResolvedValue(connection);
      api.previewTeamSync.mockRejectedValue({
        code,
        message: `synthetic ${code}`,
      });
      const view = render(<TeamPage />);

      expect(
        await screen.findByText("Retained local asset"),
      ).toBeInTheDocument();
      expect(await screen.findByText(`synthetic ${code}`)).toBeInTheDocument();

      view.unmount();
    }
  });
});
