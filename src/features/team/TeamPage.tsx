import { useTeamWorkspace, type TeamWorkspaceModel } from "./useTeamWorkspace";
import { ErrorAlert } from "./TeamFeedback";
import { ToolUsagePanel } from "./ToolUsagePanel";
import { TeamConnectSection } from "./TeamConnectSection";
import { TeamConnectionSection } from "./TeamConnectionSection";
import { TeamProviderSection } from "./TeamProviderSection";
import { TeamAssetsSection } from "./TeamAssetsSection";
import { TeamOperationStatus } from "./TeamOperationStatus";
import { useSharedTeamWorkspace } from "./TeamWorkspaceBoundary";
export type TeamSection = "all" | "organization" | "providers" | "assets";
export function TeamPage({ section = "all" }: { section?: TeamSection }) {
  const shared = useSharedTeamWorkspace();
  return shared ? (
    <TeamWorkspaceContent workspace={shared} section={section} />
  ) : (
    <StandaloneTeamPage section={section} />
  );
}
function StandaloneTeamPage({ section }: { section: TeamSection }) {
  const workspace = useTeamWorkspace();
  return <TeamWorkspaceContent workspace={workspace} section={section} />;
}
export function TeamWorkspaceContent({
  workspace,
  section = "all",
}: {
  workspace: TeamWorkspaceModel;
  section?: TeamSection;
}) {
  if (!workspace.connection)
    return (
      <main className="h-full overflow-y-auto px-6 py-8">
        <TeamOperationStatus
          busy={workspace.busy}
          operationNotice={workspace.operationNotice}
          cancelCurrentOperation={workspace.cancelCurrentOperation}
          runRefresh={workspace.runRefresh}
        />
        <TeamConnectSection
          handleConnect={workspace.handleConnect}
          handleProfileFile={workspace.handleProfileFile}
          busy={workspace.busy}
          importedProfileName={workspace.importedProfileName}
          gateway={workspace.gateway}
          setGateway={workspace.setGateway}
          memberKey={workspace.memberKey}
          setMemberKey={workspace.setMemberKey}
          error={workspace.error}
        />
      </main>
    );
  return (
    <main className="h-full overflow-y-auto px-6 py-6">
      <div className="mx-auto max-w-6xl space-y-6 pb-12">
        <TeamOperationStatus
          busy={workspace.busy}
          operationNotice={workspace.operationNotice}
          cancelCurrentOperation={workspace.cancelCurrentOperation}
          runRefresh={workspace.runRefresh}
        />
        {(section === "all" || section === "organization") && (
          <TeamConnectionSection
            connection={workspace.connection}
            statusLabel={workspace.statusLabel}
            runRefresh={workspace.runRefresh}
            busy={workspace.busy}
            setShowDisconnect={workspace.setShowDisconnect}
            showDisconnect={workspace.showDisconnect}
            removeProviderCredentials={workspace.removeProviderCredentials}
            setRemoveProviderCredentials={
              workspace.setRemoveProviderCredentials
            }
            handleDisconnect={workspace.handleDisconnect}
            models={workspace.models}
            lastSuccessfulSync={workspace.lastSuccessfulSync}
          />
        )}
        {(section === "all" || section === "organization") && (
          <ToolUsagePanel
            key={`${workspace.connection.id}:${workspace.connection.profile.key.id}`}
            disabled={workspace.busy !== null}
            onBusyChange={workspace.setReportingBusy}
          />
        )}
        {workspace.error && <ErrorAlert error={workspace.error} />}
        <section
          className={
            section === "all"
              ? "grid gap-6 lg:grid-cols-[0.9fr_1.1fr]"
              : "space-y-6"
          }
        >
          {(section === "all" || section === "providers") && (
            <TeamProviderSection
              providerApp={workspace.providerApp}
              busy={workspace.busy}
              requestRevision={workspace.requestRevision}
              setProviderApp={workspace.setProviderApp}
              setProviderPreview={workspace.setProviderPreview}
              providerApps={workspace.providerApps}
              model={workspace.model}
              setModel={workspace.setModel}
              models={workspace.models}
              providerPreview={workspace.providerPreview}
              handleProviderPreview={workspace.handleProviderPreview}
              connection={workspace.connection}
              selectedModel={workspace.selectedModel}
              handleProviderApply={workspace.handleProviderApply}
              handleProviderActivate={workspace.handleProviderActivate}
            />
          )}
          {(section === "all" || section === "assets") && (
            <TeamAssetsSection
              refreshAcknowledgements={workspace.refreshAcknowledgements}
              acknowledgements={workspace.acknowledgements}
              teamaiSync={workspace.teamaiSync}
              legacySync={workspace.legacySync}
              handleRetryAcknowledgements={
                workspace.handleRetryAcknowledgements
              }
              assetApp={workspace.assetApp}
              busy={workspace.busy}
              requestRevision={workspace.requestRevision}
              setAssetApp={workspace.setAssetApp}
              setSyncPlan={workspace.setSyncPlan}
              setSyncResult={workspace.setSyncResult}
              setOverwriteAssets={workspace.setOverwriteAssets}
              setLastSyncAt={workspace.setLastSyncAt}
              setPendingRestoreAsset={workspace.setPendingRestoreAsset}
              setLocalHistory={workspace.setLocalHistory}
              setLocalHistoryApp={workspace.setLocalHistoryApp}
              manifest={workspace.manifest}
              handleSyncPreview={workspace.handleSyncPreview}
              connection={workspace.connection}
              syncPlan={workspace.syncPlan}
              handleSync={workspace.handleSync}
              pendingRestoreAsset={workspace.pendingRestoreAsset}
              handleRestore={workspace.handleRestore}
              syncResult={workspace.syncResult}
              overwriteAssets={workspace.overwriteAssets}
              setOverwrite={workspace.setOverwrite}
              localHistoryApp={workspace.localHistoryApp}
              localHistory={workspace.localHistory}
            />
          )}
        </section>
      </div>
    </main>
  );
}
