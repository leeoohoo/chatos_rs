import { invoke } from "@tauri-apps/api/core";
import {
  projectContentSearchResultSchema,
  projectDirectoryListingSchema,
  projectEntrySearchResultSchema,
  projectFileContentSchema,
} from "../domain/projectDirectory";
import type { ProjectDirectoryLoadRequest, ProjectDirectoryPort } from "../ports/ProjectDirectoryPort";

export class TauriProjectDirectoryAdapter implements ProjectDirectoryPort {
  async loadDirectory(request: ProjectDirectoryLoadRequest) {
    return projectDirectoryListingSchema.parse(
      await invoke("load_project_directory", { ...request }),
    );
  }


  async readFile(request: Parameters<ProjectDirectoryPort["readFile"]>[0]) {
    return projectFileContentSchema.parse(await invoke("read_project_file", { ...request }));
  }

  async searchEntries(request: Parameters<ProjectDirectoryPort["searchEntries"]>[0]) {
    return projectEntrySearchResultSchema.parse(await invoke("search_project_entries", { ...request }));
  }

  async searchContent(request: Parameters<ProjectDirectoryPort["searchContent"]>[0]) {
    return projectContentSearchResultSchema.parse(await invoke("search_project_content", { ...request }));
  }
}
