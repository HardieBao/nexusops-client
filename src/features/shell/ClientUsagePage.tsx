import { UsageDashboard } from "@/components/usage/UsageDashboard";
import { ToolUsageHistoryPanel } from "@/features/team/ToolUsageHistoryPanel";
import { useSettings, type SettingsFormState } from "@/hooks/useSettings";

export function ClientUsagePage() {
  const { settings, isLoading, updateSettings, autoSaveSettings } =
    useSettings();
  async function save(updates: Partial<SettingsFormState>) {
    const result = await autoSaveSettings(updates);
    if (result) updateSettings(updates);
    return result !== null;
  }
  if (isLoading)
    return (
      <div
        aria-busy="true"
        className="m-6 h-48 animate-pulse rounded-xl bg-muted"
      />
    );
  return (
    <div className="px-6 py-5">
      <ToolUsageHistoryPanel />
      <UsageDashboard
        refreshIntervalMs={settings?.usageDashboardRefreshIntervalMs}
        onRefreshIntervalChange={(value) =>
          save({ usageDashboardRefreshIntervalMs: value })
        }
        sessionAutoSyncEnabled={settings?.sessionAutoSyncEnabled ?? true}
        onSessionAutoSyncEnabledChange={(value) =>
          save({ sessionAutoSyncEnabled: value })
        }
      />
    </div>
  );
}
