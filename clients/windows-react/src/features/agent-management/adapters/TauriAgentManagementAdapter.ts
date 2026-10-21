import { invoke } from "@tauri-apps/api/core";
import {
  agentProfileEditorCatalogSchema,
  agentProfileEditorDataSchema,
  agentProfileListSchema,
  agentProfileSummarySchema,
  agentAvatarDraftSchema,
  agentConversationActivitySnapshotSchema,
  agentProfileActivitySnapshotSchema,
  preparedAgentConversationSchema,
  type AgentProfileMutationInput,
} from "../domain/agentManagement";
import { z } from "zod";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { mediaStudioHistoryPageSchema } from "../../media-studio/domain/mediaStudio";
import {
  agentSkillCatalogSchema,
  agentSkillDefinitionSchema,
  type AgentSkillKind,
  type AgentSkillMutationInput,
} from "../domain/agentSkillManagement";

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

  async loadAvatarHistory(ownerUserId: string) {
    const page = mediaStudioHistoryPageSchema.parse(
      await invoke("load_media_studio_image_history", { ownerUserId, after: null, limit: 40 }),
    );
    return page.items.flatMap((item) => item.images.flatMap((asset) => asset.previewUrl ? [{
      recordId: item.id,
      contentToken: asset.contentToken,
      previewUrl: asset.previewUrl,
      createdAtUnixMs: item.createdAtUnixMs,
    }] : []));
  }

  async discardAvatarReferences(ownerUserId: string, presentationId: string, referenceIds: string[]) {
    await invoke("discard_media_studio_reference_images", {
      input: { ownerUserId, presentationId, referenceIds },
    });
  }

  async discardAvatarPresentation(ownerUserId: string, presentationId: string) {
    await invoke("discard_media_studio_presentation", { input: { ownerUserId, presentationId } });
  }

  async loadSkillCatalog(ownerUserId: string) {
    return agentSkillCatalogSchema.parse(
      await invoke("load_agent_skill_catalog", { ownerUserId }),
    );
  }

  async saveSkillOverride(ownerUserId: string, input: AgentSkillMutationInput) {
    return agentSkillDefinitionSchema.parse(
      await invoke("save_agent_skill_override", { ownerUserId, input }),
    );
  }

  async resetSkillOverride(ownerUserId: string, kind: AgentSkillKind, key: string) {
    return agentSkillDefinitionSchema.parse(
      await invoke("reset_agent_skill_override", { ownerUserId, kind, key }),
    );
  }

  async prepareConversation(ownerUserId: string, agentId: string) {
    return preparedAgentConversationSchema.parse(
      await invoke("prepare_agent_conversation", { ownerUserId, agentId }),
    );
  }

  async loadConversationActivity(ownerUserId: string, roomId: string) {
    return agentConversationActivitySnapshotSchema.parse(
      await invoke("load_agent_conversation_activity", { ownerUserId, roomId }),
    );
  }

  async loadProfileActivity(ownerUserId: string, agentId: string) {
    return agentProfileActivitySnapshotSchema.parse(
      await invoke("load_agent_profile_activity", { ownerUserId, agentId }),
    );
  }

  async sendConversationDraft(input: Parameters<NonNullable<AgentManagementPort["sendConversationDraft"]>>[0]) {
    return z.object({
      deliveryKind: z.enum(["guidance", "newTurn"]),
      turnId: z.string().min(1),
      userMessageId: z.string().min(1),
    }).strict().parse(await invoke("send_agent_conversation_draft", { input }));
  }
}
