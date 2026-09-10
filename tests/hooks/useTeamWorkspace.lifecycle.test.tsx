import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useTeamWorkspace } from "@/features/team/useTeamWorkspace";
import type {
  SyncBatchResult,
  SyncPlan,
  TeamConnection,
  LocalAssetHistory,
} from "@/features/team/api";

const api = vi.hoisted(() => ({
  getTeamStatus: vi.fn(),
  previewTeamSync: vi.fn(),
  previewTeamAI: vi.fn(),
  applyTeamAI: vi.fn(),
  retryTeamAIAcknowledgements: vi.fn(),
  getTeamAIAcknowledgementStatus: vi.fn(),
  exportTeamAICandidate: vi.fn(),
  getTeamLocalHistory: vi.fn(),
  cancelTeamOperation: vi.fn(),
  waitForTeamIdle: vi.fn(),
  connectTeam: vi.fn(),
  disconnectTeam: vi.fn(),
  previewTeamProvider: vi.fn(),
  applyTeamProvider: vi.fn(),
  activateTeamProvider: vi.fn(),
  restoreTeamBackup: vi.fn(),
  syncTeam: vi.fn(),
}));
vi.mock("@/features/team/api", () => api);
const dialogs = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialogs);
vi.mock("@tauri-apps/api/path", () => ({
  downloadDir: vi.fn(async () => "C:/exports"),
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
  dirname: vi.fn(async () => "C:/source"),
  basename: vi.fn(async () => "rule.md"),
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
const connection: TeamConnection = {
  id: "fixture-connection",
  gateway_url: "https://gateway.example/",
  status: "connected",
  last_checked_at: "2026-09-10T00:00:00Z",
  last_error: null,
  profile: {
    schema_version: 1,
    organization_id: "fixture-org",
    workspace_id: "fixture-workspace",
    name: "Fixture organization",
    gateway_url: "https://gateway.example/",
    base_url: "https://gateway.example/v1",
    key: {
      id: 7,
      name: "Fixture key",
      prefix: "nx_",
      last_four: "test",
      platform: "openai",
      allow_messages_dispatch: false,
    },
    models: [],
    asset_manifest_url: "/api/v1/me/assets/manifest",
    subscriptions: [],
    limits: {
      max_assets: 100,
      max_revisions_per_asset: 100,
      max_text_bytes: 1048576,
      max_archive_bytes: 20971520,
      max_unpacked_bytes: 104857600,
      max_files: 2000,
      max_path_depth: 16,
      max_path_bytes: 240,
    },
  },
};
const plan: SyncPlan = {
  connection,
  conflicts: [],
  withdrawn: [],
  last_successful_sync: null,
  items: [
    {
      asset: {
        asset_id: 1,
        kind: "rule",
        slug: "fixture-rule",
        name: "Fixture rule",
        revision: 2,
        content_hash: "a".repeat(64),
        archive_sha256: null,
        download_url: "/api/v1/assets/1/revisions/2/download",
        content_type: "text/plain",
        byte_size: 5,
        files: [],
      },
      drift: "not_installed",
      support: "managed_download",
      install_path: "fixture/rule.md",
      previous_revision: null,
      subscribed: true,
      has_backup: false,
      last_synced_at: null,
      decision_token: "synthetic-decision",
      disk_fingerprint: null,
      upstream_fingerprint: null,
      inspection_error_code: null,
      inspection_error_message: null,
    },
  ],
};
const written: LocalAssetHistory = {
  asset_id: 1,
  name: "Fixture rule",
  kind: "rule",
  revision: 2,
  last_synced_at: "2026-09-10T00:01:00Z",
  has_backup: false,
  local_file_present: true,
  recovery_error: null,
};
beforeEach(() => {
  dialogs.open.mockReset();
  dialogs.save.mockReset();
  for (const mock of Object.values(api)) mock.mockReset();
  api.previewTeamAI.mockRejectedValue({
    code: "upgrade_required",
    message: "Legacy gateway fixture",
  });
  api.getTeamStatus.mockResolvedValue(connection);
  api.getTeamAIAcknowledgementStatus.mockResolvedValue({
    connection,
    status: { pending: 0, last_error: null },
  });
  api.previewTeamSync.mockResolvedValue(plan);
  api.getTeamLocalHistory.mockResolvedValue([]);
  api.cancelTeamOperation.mockResolvedValue(undefined);
  api.waitForTeamIdle.mockResolvedValue(undefined);
  api.disconnectTeam.mockResolvedValue(undefined);
});
async function ready() {
  const hook = renderHook(() => useTeamWorkspace());
  await waitFor(() => expect(hook.result.current.syncPlan).toBe(plan));
  return hook;
}

it.each(["skill", "rule"] as const)(
  "exports a %s candidate through the shared operation state",
  async (kind) => {
    api.getTeamStatus.mockResolvedValue(null);
    dialogs.open.mockResolvedValue(
      kind === "skill" ? "C:/source" : "C:/source/rule.md",
    );
    dialogs.save.mockResolvedValue("C:/exports/candidate.nexusops-asset.json");
    api.exportTeamAICandidate.mockResolvedValue({
      output: "C:/exports/candidate.nexusops-asset.json",
      kind,
      name: "Fixture",
      content_hash: "a".repeat(64),
      bytes: 200,
    });
    const hook = renderHook(() => useTeamWorkspace());
    await act(() => hook.result.current.handleExportCandidate(kind));
    expect(api.exportTeamAICandidate).toHaveBeenCalledWith({
      kind,
      root: "C:/source",
      entry: kind === "rule" ? "rule.md" : null,
      output: "C:/exports/candidate.nexusops-asset.json",
    });
    expect(hook.result.current.candidateExport?.name).toBe("Fixture");
    expect(hook.result.current.busy).toBeNull();
    expect(api.connectTeam).not.toHaveBeenCalled();
  },
);

it("does not export when the output dialog is cancelled", async () => {
  dialogs.open.mockResolvedValue("C:/source");
  dialogs.save.mockResolvedValue(null);
  const hook = await ready();
  await act(() => hook.result.current.handleExportCandidate("skill"));
  expect(api.exportTeamAICandidate).not.toHaveBeenCalled();
  expect(hook.result.current.candidateExport).toBeNull();
  expect(hook.result.current.busy).toBeNull();
});

it("uses the signed preview for mixed assets and retries confirmation without installing again", async () => {
  const legacy = {
    ...plan.items[0],
    asset: { ...plan.items[0].asset, asset_id: 2, kind: "prompt" as const },
    support: "inactive_prompt" as const,
  };
  const modern = {
    project: {
      id: 1,
      member_id: 11,
      organization_id: "org",
      workspace_id: "workspace",
      name: "Fixture",
    },
    commands: [{ asset_id: 1, command_id: "signed-command" }],
    plan,
    legacy_items: [legacy],
  };
  api.previewTeamAI.mockResolvedValue(modern);
  api.applyTeamAI.mockResolvedValue({
    install: {
      connection,
      items: [],
      conflicts: [],
      last_successful_sync: null,
    },
    acknowledgements: {
      acknowledged: 0,
      waiting: 1,
      superseded: 0,
      last_error: "unavailable",
    },
  });
  const hook = renderHook(() => useTeamWorkspace());
  await waitFor(() =>
    expect(hook.result.current.syncPlan?.items).toHaveLength(2),
  );
  expect(hook.result.current.teamaiSync).toBe(true);
  await act(() => hook.result.current.handleSync());
  expect(api.applyTeamAI).toHaveBeenCalledWith("codex", modern, []);
  expect(api.syncTeam).not.toHaveBeenCalled();
  expect(hook.result.current.acknowledgements?.waiting).toBe(1);
  api.retryTeamAIAcknowledgements.mockResolvedValue({
    acknowledged: 1,
    waiting: 0,
    superseded: 0,
    last_error: null,
  });
  await act(() => hook.result.current.handleRetryAcknowledgements());
  expect(hook.result.current.acknowledgements?.waiting).toBe(0);
  expect(api.applyTeamAI).toHaveBeenCalledTimes(1);
  act(() => {
    hook.result.current.requestRevision.current += 1;
    hook.result.current.setAssetApp("claude");
    hook.result.current.setSyncPlan(null);
  });
  expect(hook.result.current.acknowledgements).toBeNull();
});

it("reflects automatic confirmations without issuing a retry or reusing another tool's status", async () => {
  const hook = await ready();
  api.getTeamAIAcknowledgementStatus.mockResolvedValueOnce({
    connection,
    status: { pending: 1, last_error: "unavailable" },
  });
  await act(() => hook.result.current.refreshAcknowledgements());
  expect(hook.result.current.acknowledgements?.waiting).toBe(1);
  await act(() => hook.result.current.refreshAcknowledgements());
  expect(hook.result.current.acknowledgements?.waiting).toBe(0);
  expect(api.retryTeamAIAcknowledgements).not.toHaveBeenCalled();
  const pending = deferred<{
    connection: TeamConnection;
    status: { pending: number; last_error: null };
  }>();
  api.getTeamAIAcknowledgementStatus.mockReturnValueOnce(pending.promise);
  let read!: Promise<void>;
  act(() => {
    read = hook.result.current.refreshAcknowledgements();
  });
  act(() => {
    hook.result.current.requestRevision.current += 1;
    hook.result.current.setAssetApp("claude");
    hook.result.current.setSyncPlan(null);
  });
  await act(async () => {
    pending.resolve({ connection, status: { pending: 9, last_error: null } });
    await read;
  });
  expect(hook.result.current.acknowledgements).toBeNull();
});

it("does not downgrade authenticated or network failures to legacy synchronization", async () => {
  api.previewTeamAI.mockRejectedValue({
    code: "authentication_required",
    message: "Reconnect",
  });
  const hook = renderHook(() => useTeamWorkspace());
  await waitFor(() =>
    expect(hook.result.current.error?.code).toBe("authentication_required"),
  );
  expect(api.previewTeamSync).not.toHaveBeenCalled();
});

it("does not start a legacy request after a stale capability response", async () => {
  const capability = deferred<never>();
  api.previewTeamAI.mockReturnValue(capability.promise);
  const hook = renderHook(() => useTeamWorkspace());
  await waitFor(() => expect(api.previewTeamAI).toHaveBeenCalled());
  act(() => {
    hook.result.current.requestRevision.current += 1;
  });
  await act(async () =>
    capability.reject({ code: "upgrade_required", message: "Legacy" }),
  );
  expect(api.previewTeamSync).not.toHaveBeenCalled();
});

describe("Team operation cancellation and stale responses", () => {
  it("does not unlock after the JS call settles until the native idle barrier confirms", async () => {
    const { result } = await ready();
    const request = deferred<SyncPlan>(),
      idle = deferred<void>();
    api.previewTeamSync.mockReturnValueOnce(request.promise);
    api.waitForTeamIdle.mockReturnValueOnce(idle.promise);
    let original!: Promise<void>;
    let stopped!: Promise<void>;
    act(() => {
      original = result.current.runRefresh();
    });
    await waitFor(() => expect(api.previewTeamSync).toHaveBeenCalledTimes(2));
    act(() => {
      stopped = result.current.cancelCurrentOperation();
    });
    await act(async () => {
      request.resolve(plan);
      await original;
    });
    await waitFor(() => expect(api.waitForTeamIdle).toHaveBeenCalledTimes(1));
    expect(result.current.busy).toBe("cancelling");
    expect(api.getTeamStatus).toHaveBeenCalledTimes(1);
    await act(async () => {
      idle.resolve();
      await stopped;
    });
    expect(result.current.busy).toBeNull();
  });
  it("waits for the owned call, ignores its late error, and blocks new work while stopping", async () => {
    const { result } = await ready();
    const request = deferred<SyncPlan>();
    api.previewTeamSync.mockReturnValueOnce(request.promise);
    let original!: Promise<void>;
    let stopped!: Promise<void>;
    act(() => {
      original = result.current.runRefresh();
    });
    await waitFor(() => expect(api.previewTeamSync).toHaveBeenCalledTimes(2));
    act(() => {
      stopped = result.current.cancelCurrentOperation();
    });
    await waitFor(() => expect(result.current.busy).toBe("cancelling"));
    await act(async () => {
      await result.current.runRefresh();
    });
    expect(api.previewTeamSync).toHaveBeenCalledTimes(2);
    expect(api.getTeamStatus).toHaveBeenCalledTimes(1);
    await act(async () => {
      request.reject({ code: "timeout", message: "PRIVATE_OLD_ERROR" });
      await original;
      await stopped;
    });
    expect(result.current.busy).toBeNull();
    expect(result.current.error).toBeNull();
    expect(result.current.operationNotice).toBe("stopped");
    expect(api.getTeamStatus).toHaveBeenCalledTimes(2);
    expect(result.current.syncPlan).toBeNull();
  });
  it("keeps completed write records instead of claiming that cancellation rolled them back", async () => {
    const { result } = await ready();
    const request = deferred<SyncBatchResult>();
    api.syncTeam.mockReturnValueOnce(request.promise);
    let original!: Promise<void>;
    let stopped!: Promise<void>;
    act(() => {
      original = result.current.handleSync();
    });
    act(() => {
      stopped = result.current.cancelCurrentOperation();
    });
    await waitFor(() => expect(result.current.busy).toBe("cancelling"));
    api.getTeamLocalHistory.mockResolvedValue([written]);
    await act(async () => {
      request.resolve({
        connection,
        conflicts: [],
        items: [
          {
            asset_id: 1,
            revision: 2,
            outcome: "downloaded",
            drift: "not_installed",
            install_path: "fixture/rule.md",
            backup_path: null,
            error_code: null,
            error_message: null,
          },
        ],
        last_successful_sync: written.last_synced_at,
      });
      await original;
      await stopped;
    });
    expect(result.current.localHistory).toEqual([written]);
    expect(result.current.syncResult).toBeNull();
    expect(result.current.operationNotice).toBe("stopped");
    expect(api.previewTeamSync).toHaveBeenCalledTimes(1);
    expect(api.restoreTeamBackup).not.toHaveBeenCalled();
  });
  it("does not restore an old connection when its recovery read arrives after disconnect", async () => {
    const { result } = await ready();
    const stale = deferred<TeamConnection>();
    api.previewTeamSync.mockRejectedValueOnce({
      code: "timeout",
      message: "fixture error",
    });
    api.getTeamStatus.mockReturnValueOnce(stale.promise);
    let refresh!: Promise<void>;
    act(() => {
      refresh = result.current.runRefresh();
    });
    await waitFor(() => expect(api.getTeamStatus).toHaveBeenCalledTimes(2));
    await act(async () => {
      await result.current.handleDisconnect();
    });
    expect(result.current.connection).toBeNull();
    await act(async () => {
      stale.resolve(connection);
      await refresh;
    });
    expect(result.current.connection).toBeNull();
    expect(result.current.error).toBeNull();
    expect(result.current.busy).toBeNull();
    expect(result.current.memberKey).toBe("");
  });
  it("reports a failed cancellation signal only after the current call settles", async () => {
    const { result } = await ready();
    const request = deferred<SyncPlan>();
    api.previewTeamSync.mockReturnValueOnce(request.promise);
    api.cancelTeamOperation.mockRejectedValueOnce(
      new Error("PRIVATE_TRANSPORT_ERROR"),
    );
    let original!: Promise<void>;
    let stopped!: Promise<void>;
    act(() => {
      original = result.current.runRefresh();
    });
    await waitFor(() => expect(api.previewTeamSync).toHaveBeenCalledTimes(2));
    act(() => {
      stopped = result.current.cancelCurrentOperation();
    });
    await waitFor(() => expect(result.current.busy).toBe("cancelling"));
    await act(async () => {
      request.resolve(plan);
      await original;
      await stopped;
    });
    expect(result.current.operationNotice).toBe("cancelFailed");
    expect(result.current.error).toBeNull();
    expect(result.current.busy).toBeNull();
  });
  it("reports reconciliation failure without exposing raw errors or asserting rollback", async () => {
    const { result } = await ready();
    const request = deferred<SyncPlan>();
    api.previewTeamSync.mockReturnValueOnce(request.promise);
    let original!: Promise<void>;
    let stopped!: Promise<void>;
    act(() => {
      original = result.current.runRefresh();
    });
    await waitFor(() => expect(api.previewTeamSync).toHaveBeenCalledTimes(2));
    act(() => {
      stopped = result.current.cancelCurrentOperation();
    });
    api.getTeamStatus.mockRejectedValueOnce(new Error("PRIVATE_STORAGE_ERROR"));
    await act(async () => {
      request.resolve(plan);
      await original;
      await stopped;
    });
    expect(result.current.operationNotice).toBe("recoveryFailed");
    expect(result.current.error).toBeNull();
    expect(result.current.syncPlan).toBeNull();
    expect(result.current.busy).toBeNull();
  });
  it("aborts a pending Profile read and keeps credentials empty", async () => {
    api.getTeamStatus.mockResolvedValue(null);
    const { result } = renderHook(() => useTeamWorkspace());
    await act(async () => {
      await Promise.resolve();
    });
    const read = vi
      .spyOn(FileReader.prototype, "readAsText")
      .mockImplementation(() => {});
    const abort = vi.spyOn(FileReader.prototype, "abort");
    const input = document.createElement("input");
    input.type = "file";
    Object.defineProperty(input, "files", {
      value: [new File(['{"member_key":"PRIVATE"}'], "profile.json")],
    });
    let original!: Promise<void>;
    let stopped!: Promise<void>;
    act(() => {
      original = result.current.handleProfileFile({
        currentTarget: input,
      } as React.ChangeEvent<HTMLInputElement>);
    });
    act(() => {
      stopped = result.current.cancelCurrentOperation();
    });
    await act(async () => {
      await original;
      await stopped;
    });
    expect(abort).toHaveBeenCalled();
    expect(result.current.memberKey).toBe("");
    expect(result.current.importedProfileName).toBeNull();
    expect(result.current.operationNotice).toBe("stopped");
    read.mockRestore();
    abort.mockRestore();
  });
});
