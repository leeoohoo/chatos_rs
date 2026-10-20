import { invoke } from "@tauri-apps/api/core";
import { projectDirectoryListingSchema } from "../domain/projectDirectory";
import type { ProjectDirectoryLoadRequest, ProjectDirectoryPort } from "../ports/ProjectDirectoryPort";

export class TauriProjectDirectoryAdapter implements ProjectDirectoryPort {
  async loadDirectory(request: ProjectDirectoryLoadRequest) {
    return projectDirectoryListingSchema.parse(
      await invoke("load_project_directory", { ...request }),
    );
  }
}
