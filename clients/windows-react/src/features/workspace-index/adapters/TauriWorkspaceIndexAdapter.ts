import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import {
  localProjectSchema,
  workspaceRelationsSchema,
  type LocalProject,
  type WorkspaceRelations,
} from "../domain/workspaceIndex";
import type { WorkspaceIndexPort } from "../ports/WorkspaceIndexPort";

export class TauriWorkspaceIndexAdapter implements WorkspaceIndexPort {
  async listLocalProjects(ownerUserId: string): Promise<LocalProject[]> {
    return z.array(localProjectSchema).parse(
      await invoke("list_local_projects", { ownerUserId }),
    );
  }

  async refreshRelations(ownerUserId: string): Promise<WorkspaceRelations> {
    return workspaceRelationsSchema.parse(
      await invoke("refresh_workspace_relations", { ownerUserId }),
    );
  }
}

