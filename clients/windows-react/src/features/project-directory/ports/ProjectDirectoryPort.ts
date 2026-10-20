import type {
  ProjectContentSearchResult,
  ProjectDirectoryListing,
  ProjectEntrySearchResult,
  ProjectFileContent,
  ProjectFileWriteResult,
  ProjectEntryMutationResult,
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
  writeFile(request: ProjectFileWriteRequest): Promise<ProjectFileWriteResult>;
  createEntry(request: ProjectEntryCreateRequest): Promise<ProjectEntryMutationResult>;
  deleteEntry(request: ProjectEntryDeleteRequest): Promise<ProjectEntryMutationResult>;
}

export interface ProjectFileWriteRequest extends ProjectFileRequest {
  expectedRevision: string;
  content: string;
}

export interface ProjectEntryCreateRequest {
  ownerUserId: string;
  projectId: string;
  parentPath: string;
  name: string;
  kind: "file" | "directory";
}

export interface ProjectEntryDeleteRequest extends ProjectFileRequest {
  kind: "file" | "directory";
  expectedRevision: string | null;
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
