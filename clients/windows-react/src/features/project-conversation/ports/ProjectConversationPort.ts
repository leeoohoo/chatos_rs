import type { PreparedProjectConversation } from "../domain/projectConversation";

export interface ProjectConversationPort {
  prepareProjectConversation(
    ownerUserId: string,
    projectId: string,
  ): Promise<PreparedProjectConversation>;
}
