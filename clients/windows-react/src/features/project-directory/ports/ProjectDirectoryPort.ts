import type {
  ProjectContentSearchResult,
  ProjectDirectoryListing,
  ProjectEntrySearchResult,
  ProjectFileContent,
} from "../domain/projectDirectory";

export interface ProjectDirectoryLoadRequest {
  ownerUserId: string;
  projectId: string;
  path: string;
  forceRefresh: boolean;
}

export interface ProjectDirectoryPort {
  loadDirectory(request: ProjectDirectoryLoadRequest): Promise<ProjectDirectoryListing>;
  readFile(request: ProjectFileRequest): Promise<ProjectFileContent>;
  searchEntries(request: ProjectSearchRequest): Promise<ProjectEntrySearchResult>;
  searchContent(request: ProjectSearchRequest): Promise<ProjectContentSearchResult>;
}

export interface ProjectFileRequest {
  ownerUserId: string;
  projectId: string;
  path: string;
}

export interface ProjectSearchRequest {
  ownerUserId: string;
  projectId: string;
  path: string;
  query: string;
  limit: number;
}
