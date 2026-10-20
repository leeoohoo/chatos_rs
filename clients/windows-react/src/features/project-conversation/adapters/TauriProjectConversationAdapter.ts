import { invoke } from "@tauri-apps/api/core";
import { preparedProjectConversationSchema } from "../domain/projectConversation";
import type { ProjectConversationPort } from "../ports/ProjectConversationPort";

export class TauriProjectConversationAdapter implements ProjectConversationPort {
  async prepareProjectConversation(ownerUserId: string, projectId: string) {
    return preparedProjectConversationSchema.parse(
      await invoke("prepare_project_conversation", { ownerUserId, projectId }),
    );
  }
}
