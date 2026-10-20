import type { ProjectDirectoryListing } from "../domain/projectDirectory";

export interface ProjectDirectoryLoadRequest {
  ownerUserId: string;
  projectId: string;
  path: string;
  forceRefresh: boolean;
}

export interface ProjectDirectoryPort {
  loadDirectory(request: ProjectDirectoryLoadRequest): Promise<ProjectDirectoryListing>;
}
