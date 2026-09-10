import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  type ReactNode,
} from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTeamWorkspace, type TeamWorkspaceModel } from "./useTeamWorkspace";

const TeamWorkspaceContext = createContext<TeamWorkspaceModel | null>(null);
export function useSharedTeamWorkspace() {
  return useContext(TeamWorkspaceContext);
}
interface Props {
  children: ReactNode;
  active: boolean;
  initialApp: string;
  onBusyChange: (busy: boolean) => void;
}
function SharedWorkspace({
  children,
  initialApp,
  onBusyChange,
}: Omit<Props, "active">) {
  const workspace = useTeamWorkspace(initialApp);
  const queryClient = useQueryClient();
  useEffect(() => {
    queryClient.setQueryData(["team", "status"], workspace.connection);
  }, [queryClient, workspace.connection]);
  useLayoutEffect(() => {
    onBusyChange(workspace.busy !== null);
    return () => onBusyChange(false);
  }, [workspace.busy, onBusyChange]);
  return (
    <TeamWorkspaceContext.Provider value={workspace}>
      {children}
    </TeamWorkspaceContext.Provider>
  );
}
// Remains outside the keyed page outlet so related Team pages share one operation owner.
export function TeamWorkspaceBoundary({ active, children, ...props }: Props) {
  return active ? (
    <SharedWorkspace {...props}>{children}</SharedWorkspace>
  ) : (
    <>{children}</>
  );
}
