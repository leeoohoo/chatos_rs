import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import type { ConversationRuntimeSettingsPort } from "../ports/ConversationRuntimeSettingsPort";

const snapshotSchema = z.object({
  settings: z.object({
    selectedModelId: z.string().min(1),
    selectedThinkingLevel: z.string().nullable(),
    remoteConnectionId: z.string().nullable(),
    reasoningEnabled: z.boolean(),
    version: z.number().int().positive(),
  }),
  models: z.array(z.object({
    id: z.string().min(1),
    displayName: z.string().min(1),
    modelName: z.string().min(1),
    provider: z.string().min(1),
    defaultThinkingLevel: z.string().min(1),
    supportsReasoning: z.boolean(),
    thinkingLevels: z.array(z.string().min(1)),
  })).min(1),
  remoteConnections: z.array(z.object({
    id: z.string().min(1),
    name: z.string().min(1),
  })),
});

export class TauriConversationRuntimeSettingsAdapter
implements ConversationRuntimeSettingsPort {
  async load(scope: Parameters<ConversationRuntimeSettingsPort["load"]>[0]) {
    return snapshotSchema.parse(await invoke("load_conversation_runtime_settings", { ...scope }));
  }

  async selectModel(input: Parameters<ConversationRuntimeSettingsPort["selectModel"]>[0]) {
    return snapshotSchema.parse(await invoke("select_conversation_model", { ...input }));
  }

  async selectReasoningLevel(
    input: Parameters<ConversationRuntimeSettingsPort["selectReasoningLevel"]>[0],
  ) {
    return snapshotSchema.parse(await invoke("select_conversation_reasoning_level", { ...input }));
  }

  async selectRemoteConnection(
    input: Parameters<ConversationRuntimeSettingsPort["selectRemoteConnection"]>[0],
  ) {
    return snapshotSchema.parse(await invoke("select_conversation_remote_connection", { ...input }));
  }
}
