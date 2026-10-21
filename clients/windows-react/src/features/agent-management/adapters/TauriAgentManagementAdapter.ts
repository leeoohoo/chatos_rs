import { invoke } from "@tauri-apps/api/core";
import { agentProfileListSchema } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

export class TauriAgentManagementAdapter implements AgentManagementPort {
  async listProfiles(ownerUserId: string) {
    return agentProfileListSchema.parse(
      await invoke("list_agent_profiles", { ownerUserId }),
    );
  }
}
