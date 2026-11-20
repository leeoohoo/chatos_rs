import type { WorkspaceIndexSnapshot, WorkspaceSelection } from "../features/workspace-index/domain/workspaceIndex";

/**
 * Select the first trusted resource after authentication without overriding an
 * explicit surface or a user-selected workspace item.
 */
export function selectAuthenticatedWorkspace(
  selection: WorkspaceSelection,
  activeSurface: "workspace" | string,
  ownerUserId: string,
  workspaceOwnerUserId: string | null,
  snapshot: WorkspaceIndexSnapshot,
): WorkspaceSelection {
  if (
    activeSurface !== "workspace"
    || selection.kind !== "workbench"
    || workspaceOwnerUserId !== ownerUserId
  ) {
    return selection;
  }
  const firstProject = snapshot.projects[0];
  if (firstProject) return { kind: "project", id: firstProject.id };
  const firstContact = snapshot.contacts[0];
  if (firstContact) return { kind: "contact", id: firstContact.id };
  return selection;
}
