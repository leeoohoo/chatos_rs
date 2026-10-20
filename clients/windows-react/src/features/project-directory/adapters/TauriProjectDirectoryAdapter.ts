import { invoke } from "@tauri-apps/api/core";
import {
  projectContentSearchResultSchema,
  projectDirectoryListingSchema,
  projectEntrySearchResultSchema,
  projectFileContentSchema,
  projectFileWriteResultSchema,
  projectEntryMutationResultSchema,
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

  async writeFile(request: Parameters<ProjectDirectoryPort["writeFile"]>[0]) {
    return projectFileWriteResultSchema.parse(await invoke("write_project_file", { ...request }));
  }

  async createEntry(request: Parameters<ProjectDirectoryPort["createEntry"]>[0]) {
    return projectEntryMutationResultSchema.parse(await invoke("create_project_entry", { ...request }));
  }

  async deleteEntry(request: Parameters<ProjectDirectoryPort["deleteEntry"]>[0]) {
    return projectEntryMutationResultSchema.parse(await invoke("delete_project_entry", { ...request }));
  }
}
