import type {
  LocalProject,
  WorkspaceRelations,
} from "../domain/workspaceIndex";

export interface WorkspaceIndexPort {
  listLocalProjects(ownerUserId: string): Promise<LocalProject[]>;
  refreshRelations(ownerUserId: string): Promise<WorkspaceRelations>;
}

