import type {
  CreateLocalProjectInput,
  CreatedLocalProject,
  ProjectDirectorySelection,
  ProjectTypeDefinition,
} from "../domain/projectCreation";

export interface ProjectCreationPort {
  listProjectTypes(): Promise<ProjectTypeDefinition[]>;
  pickProjectDirectory(
    ownerUserId: string,
    presentationId: string,
  ): Promise<ProjectDirectorySelection | null>;
  discardProjectDirectorySelection(
    ownerUserId: string,
    presentationId: string,
  ): Promise<void>;
  createLocalProject(input: CreateLocalProjectInput): Promise<CreatedLocalProject>;
}
