import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import {
  createdLocalProjectSchema,
  projectDirectorySelectionSchema,
  projectTypeDefinitionSchema,
  type CreateLocalProjectInput,
  type CreatedLocalProject,
  type ProjectDirectorySelection,
  type ProjectTypeDefinition,
} from "../domain/projectCreation";
import type { ProjectCreationPort } from "../ports/ProjectCreationPort";

export class TauriProjectCreationAdapter implements ProjectCreationPort {
  async listProjectTypes(): Promise<ProjectTypeDefinition[]> {
    return z.array(projectTypeDefinitionSchema).parse(await invoke("list_project_types"));
  }

  async pickProjectDirectory(
    ownerUserId: string,
    presentationId: string,
  ): Promise<ProjectDirectorySelection | null> {
    return projectDirectorySelectionSchema.nullable().parse(
      await invoke("pick_project_directory", { ownerUserId, presentationId }),
    );
  }

  async discardProjectDirectorySelection(
    ownerUserId: string,
    presentationId: string,
  ): Promise<void> {
    await invoke("discard_project_directory_selection", { ownerUserId, presentationId });
  }

  async createLocalProject(input: CreateLocalProjectInput): Promise<CreatedLocalProject> {
    return createdLocalProjectSchema.parse(await invoke("create_local_project", {
      ownerUserId: input.ownerUserId,
      presentationId: input.presentationId,
      selectionId: input.selectionId,
      name: input.name,
      description: input.description,
      projectTypeKey: input.projectTypeKey,
    }));
  }
}
