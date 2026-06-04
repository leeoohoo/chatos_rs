export interface SessionRuntimeMetadata {
  contactAgentId: string | null;
  contactId: string | null;
  remoteConnectionId: string | null;
  selectedModelId: string | null;
  selectedModelName: string | null;
  selectedThinkingLevel: string | null;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  autoCreateTask: boolean;
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

const getSessionMetadataSource = (metadata: unknown): MetadataRecord => {
  const meta = parseSessionMetadata(metadata);
  const source = asMetadataRecord(meta.source_metadata);
  return Object.keys(source).length > 0 ? source : meta;
};

const getMutableSessionMetadataSource = (metadata: MetadataRecord): MetadataRecord => {
  const source = asMetadataRecord(metadata.source_metadata);
  return Object.keys(source).length > 0
    ? { ...source }
    : { ...metadata };
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
  const meta = getSessionMetadataSource(metadata);
  const runtime = asMetadataRecord(meta.chat_runtime);
  const contact = asMetadataRecord(meta.contact);
  const uiContact = asMetadataRecord(meta.ui_contact);

  const selectedModelId = normalizeId(
    runtime.selected_model_id ?? runtime.selectedModelId,
  ) || readUiChatSelectionModelId(meta);
  const selectedModelName = normalizeId(
    runtime.selected_model_name ?? runtime.selectedModelName,
  );
  const selectedThinkingLevel = normalizeId(
    runtime.selected_thinking_level ?? runtime.selectedThinkingLevel,
  );

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
  const mcpEnabledRaw = runtime.mcp_enabled ?? runtime.mcpEnabled;
  const mcpEnabled = typeof mcpEnabledRaw === 'boolean' ? mcpEnabledRaw : true;
  const enabledMcpIds = normalizeIdArray(runtime.enabled_mcp_ids ?? runtime.enabledMcpIds);
  const autoCreateTaskRaw = runtime.auto_create_task ?? runtime.autoCreateTask;
  const autoCreateTask = typeof autoCreateTaskRaw === 'boolean' ? autoCreateTaskRaw : false;

  if (
    !selectedModelId
    && !selectedModelName
    && !selectedThinkingLevel
    && !contactAgentId
    && !contactId
    && !remoteConnectionId
    && !projectId
    && !projectRoot
    && !workspaceRoot
    && enabledMcpIds.length === 0
    && mcpEnabled
    && !autoCreateTask
  ) {
    return null;
  }

  return {
    contactAgentId,
    contactId,
    remoteConnectionId,
    selectedModelId,
    selectedModelName,
    selectedThinkingLevel,
    mcpEnabled,
    enabledMcpIds,
    autoCreateTask,
    projectId,
    projectRoot,
    workspaceRoot,
  };
};

export const mergeSessionRuntimeIntoMetadata = (
  metadata: unknown,
  runtime: Partial<SessionRuntimeMetadata>,
): MetadataRecord => {
  const next = parseSessionMetadata(metadata);
  const source = getMutableSessionMetadataSource(next);
  const existingRuntime = readSessionRuntimeFromMetadata(next);

  const hasOwn = (key: keyof SessionRuntimeMetadata): boolean => (
    Object.prototype.hasOwnProperty.call(runtime, key)
  );
  const selectedModelId = normalizeId(
    hasOwn('selectedModelId') ? runtime.selectedModelId : existingRuntime?.selectedModelId,
  );
  const selectedModelName = normalizeId(
    hasOwn('selectedModelName') ? runtime.selectedModelName : existingRuntime?.selectedModelName,
  );
  const selectedThinkingLevel = normalizeId(
    hasOwn('selectedThinkingLevel')
      ? runtime.selectedThinkingLevel
      : existingRuntime?.selectedThinkingLevel,
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
  const autoCreateTask = typeof runtime.autoCreateTask === 'boolean'
    ? runtime.autoCreateTask
    : (existingRuntime?.autoCreateTask ?? false);

  source.chat_runtime = {
    selected_model_id: selectedModelId,
    selected_model_name: selectedModelName,
    selected_thinking_level: selectedThinkingLevel,
    contact_agent_id: contactAgentId,
    remote_connection_id: remoteConnectionId,
    mcp_enabled: mcpEnabled,
    enabled_mcp_ids: enabledMcpIds,
    auto_create_task: autoCreateTask,
    project_id: projectId,
    project_root: projectRoot,
    workspace_root: workspaceRoot,
  };
  source.contact = {
    type: 'memory_agent',
    agent_id: contactAgentId,
    contact_id: contactId,
  };
  source.ui_chat_selection = {
    selected_model_id: selectedModelId,
    selected_model_name: selectedModelName,
    selected_thinking_level: selectedThinkingLevel,
    selected_agent_id: contactAgentId,
  };
  source.ui_contact = {
    type: 'memory_agent',
    agent_id: contactAgentId,
    contact_id: contactId,
  };

  if (Object.keys(asMetadataRecord(next.source_metadata)).length > 0) {
    next.source_metadata = source;
  } else {
    Object.assign(next, source);
  }
  return next;
};
