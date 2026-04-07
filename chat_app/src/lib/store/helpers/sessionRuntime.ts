export interface SessionRuntimeMetadata {
  contactAgentId: string | null;
  contactId: string | null;
  remoteConnectionId: string | null;
  selectedModelId: string | null;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  projectId: string | null;
  projectRoot: string | null;
  workspaceRoot: string | null;
}

type MetadataRecord = Record<string, unknown>;

const normalizeId = (value: unknown): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

const normalizeIdArray = (value: unknown): string[] => {
  if (!Array.isArray(value)) {
    return [];
  }
  const out: string[] = [];
  for (const item of value) {
    const normalized = normalizeId(item);
    if (!normalized) continue;
    if (out.includes(normalized)) continue;
    out.push(normalized);
  }
  return out;
};

const asMetadataRecord = (value: unknown): MetadataRecord => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as MetadataRecord
    : {}
);

const parseSessionMetadata = (metadata: unknown): MetadataRecord => {
  if (metadata && typeof metadata === 'object' && !Array.isArray(metadata)) {
    return { ...(metadata as MetadataRecord) };
  }
  if (typeof metadata === 'string') {
    try {
      const parsed = JSON.parse(metadata);
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        return parsed as MetadataRecord;
      }
    } catch {
      // ignore parse errors and fallback to empty object
    }
  }
  return {};
};

const readUiChatSelectionModelId = (metadata: MetadataRecord): string | null => {
  const uiChatSelection = asMetadataRecord(metadata.ui_chat_selection);
  if (!uiChatSelection || typeof uiChatSelection !== 'object' || Array.isArray(uiChatSelection)) {
    return null;
  }
  return normalizeId(uiChatSelection.selected_model_id ?? uiChatSelection.selectedModelId);
};

const readUiChatSelectionAgentId = (metadata: MetadataRecord): string | null => {
  const uiChatSelection = asMetadataRecord(metadata.ui_chat_selection);
  if (!uiChatSelection || typeof uiChatSelection !== 'object' || Array.isArray(uiChatSelection)) {
    return null;
  }
  return normalizeId(uiChatSelection.selected_agent_id ?? uiChatSelection.selectedAgentId);
};

export const readSessionRuntimeFromMetadata = (
  metadata: unknown,
): SessionRuntimeMetadata | null => {
  const meta = parseSessionMetadata(metadata);
  const runtime = asMetadataRecord(meta.chat_runtime);
  const contact = asMetadataRecord(meta.contact);
  const uiContact = asMetadataRecord(meta.ui_contact);

  const selectedModelId = normalizeId(
    runtime.selected_model_id ?? runtime.selectedModelId,
  ) || readUiChatSelectionModelId(meta);

  const contactAgentId = normalizeId(
    contact.agent_id
      ?? contact.agentId
      ?? runtime.contact_agent_id
      ?? runtime.contactAgentId
      ?? uiContact.agent_id
      ?? uiContact.agentId,
  ) || readUiChatSelectionAgentId(meta);
  const contactId = normalizeId(
    contact.contact_id ?? contact.contactId ?? uiContact.contact_id ?? uiContact.contactId,
  );
  const remoteConnectionId = normalizeId(
    runtime.remote_connection_id ?? runtime.remoteConnectionId,
  );

  const projectId = normalizeId(
    runtime.project_id ?? runtime.projectId,
  );
  const projectRoot = normalizeId(
    runtime.project_root ?? runtime.projectRoot,
  );
  const workspaceRoot = normalizeId(
    runtime.workspace_root ?? runtime.workspaceRoot,
  );
  const usesFixedContactBuiltinProfile = Boolean(contactAgentId);
  const mcpEnabledRaw = runtime.mcp_enabled ?? runtime.mcpEnabled;
  const mcpEnabled = usesFixedContactBuiltinProfile
    ? true
    : (typeof mcpEnabledRaw === 'boolean' ? mcpEnabledRaw : true);
  const enabledMcpIds = usesFixedContactBuiltinProfile
    ? []
    : normalizeIdArray(runtime.enabled_mcp_ids ?? runtime.enabledMcpIds);

  if (
    !selectedModelId
    && !contactAgentId
    && !contactId
    && !remoteConnectionId
    && !projectId
    && !projectRoot
    && !workspaceRoot
    && enabledMcpIds.length === 0
    && mcpEnabled
  ) {
    return null;
  }

  return {
    contactAgentId,
    contactId,
    remoteConnectionId,
    selectedModelId,
    mcpEnabled,
    enabledMcpIds,
    projectId,
    projectRoot,
    workspaceRoot,
  };
};

export const readSessionImConversationId = (metadata: unknown): string | null => {
  const meta = parseSessionMetadata(metadata);
  const im = asMetadataRecord(meta.im);
  return normalizeId(im.conversation_id ?? im.conversationId);
};

export const mergeSessionRuntimeIntoMetadata = (
  metadata: unknown,
  runtime: Partial<SessionRuntimeMetadata>,
): MetadataRecord => {
  const next = parseSessionMetadata(metadata);
  const existingRuntime = readSessionRuntimeFromMetadata(next);

  const hasOwn = (key: keyof SessionRuntimeMetadata): boolean => (
    Object.prototype.hasOwnProperty.call(runtime, key)
  );
  const selectedModelId = normalizeId(
    hasOwn('selectedModelId') ? runtime.selectedModelId : existingRuntime?.selectedModelId,
  );
  const contactAgentId = normalizeId(
    hasOwn('contactAgentId') ? runtime.contactAgentId : existingRuntime?.contactAgentId,
  );
  const contactId = normalizeId(
    hasOwn('contactId') ? runtime.contactId : existingRuntime?.contactId,
  );
  const remoteConnectionId = normalizeId(
    hasOwn('remoteConnectionId') ? runtime.remoteConnectionId : existingRuntime?.remoteConnectionId,
  );
  const projectId = normalizeId(
    hasOwn('projectId') ? runtime.projectId : existingRuntime?.projectId,
  );
  const projectRoot = normalizeId(
    hasOwn('projectRoot') ? runtime.projectRoot : existingRuntime?.projectRoot,
  );
  const workspaceRoot = normalizeId(
    hasOwn('workspaceRoot') ? runtime.workspaceRoot : existingRuntime?.workspaceRoot,
  );
  const mcpEnabled = typeof runtime.mcpEnabled === 'boolean'
    ? runtime.mcpEnabled
    : (existingRuntime?.mcpEnabled ?? true);
  const enabledMcpIds = runtime.enabledMcpIds
    ? normalizeIdArray(runtime.enabledMcpIds)
    : (existingRuntime?.enabledMcpIds ?? []);
  const usesFixedContactBuiltinProfile = Boolean(contactAgentId);

  next.chat_runtime = {
    selected_model_id: selectedModelId,
    contact_agent_id: contactAgentId,
    remote_connection_id: remoteConnectionId,
    project_id: projectId,
    project_root: projectRoot,
    workspace_root: workspaceRoot,
  };
  if (!usesFixedContactBuiltinProfile) {
    (next.chat_runtime as MetadataRecord).mcp_enabled = mcpEnabled;
    (next.chat_runtime as MetadataRecord).enabled_mcp_ids = enabledMcpIds;
  }
  next.contact = {
    type: 'memory_agent',
    agent_id: contactAgentId,
    contact_id: contactId,
  };
  next.ui_chat_selection = {
    selected_model_id: selectedModelId,
    selected_agent_id: contactAgentId,
  };
  next.ui_contact = {
    type: 'memory_agent',
    agent_id: contactAgentId,
    contact_id: contactId,
  };
  return next;
};
