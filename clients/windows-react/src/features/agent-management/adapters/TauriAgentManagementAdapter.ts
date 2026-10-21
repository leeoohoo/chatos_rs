import { invoke } from "@tauri-apps/api/core";
import {
  agentProfileEditorCatalogSchema,
  agentProfileEditorDataSchema,
  agentProfileListSchema,
  agentProfileSummarySchema,
  type AgentProfileMutationInput,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

export class TauriAgentManagementAdapter implements AgentManagementPort {
  async listProfiles(ownerUserId: string) {
    return agentProfileListSchema.parse(
      await invoke("list_agent_profiles", { ownerUserId }),
    );
  }

  async loadEditorCatalog(ownerUserId: string) {
    return agentProfileEditorCatalogSchema.parse(
      await invoke("load_agent_profile_editor_catalog", { ownerUserId }),
    );
  }

  async loadProfileEditor(ownerUserId: string, agentId: string) {
    return agentProfileEditorDataSchema.parse(
      await invoke("load_agent_profile_editor", { ownerUserId, agentId }),
    );
  }

  async saveProfile(ownerUserId: string, input: AgentProfileMutationInput) {
    return agentProfileSummarySchema.parse(
      await invoke("save_agent_profile", { ownerUserId, input }),
    );
  }
}
