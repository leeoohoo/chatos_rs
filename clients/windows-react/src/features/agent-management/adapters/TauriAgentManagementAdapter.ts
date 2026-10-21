import { invoke } from "@tauri-apps/api/core";
import {
  agentProfileEditorCatalogSchema,
  agentProfileEditorDataSchema,
  agentProfileListSchema,
  agentProfileSummarySchema,
  agentAvatarDraftSchema,
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

  async pickAvatar(ownerUserId: string, presentationId: string) {
    return agentAvatarDraftSchema.nullable().parse(
      await invoke("pick_agent_avatar", { input: { ownerUserId, presentationId } }),
    );
  }

  async discardAvatarReferences(ownerUserId: string, presentationId: string, referenceIds: string[]) {
    await invoke("discard_media_studio_reference_images", {
      input: { ownerUserId, presentationId, referenceIds },
    });
  }

  async discardAvatarPresentation(ownerUserId: string, presentationId: string) {
    await invoke("discard_media_studio_presentation", { input: { ownerUserId, presentationId } });
  }
}
