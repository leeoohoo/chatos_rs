export interface SessionRuntimeMetadata {
  contactAgentId: string | null;
  contactId: string | null;
  selectedModelId: string | null;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  projectId: string | null;
  projectRoot: string | null;
  workspaceRoot: string | null;
}

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

const parseSessionMetadata = (metadata: any): Record<string, any> => {
  if (metadata && typeof metadata === 'object' && !Array.isArray(metadata)) {
    return { ...metadata };
  }
  if (typeof metadata === 'string') {
    try {
      const parsed = JSON.parse(metadata);
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        return parsed;
      }
    } catch {
      // ignore parse errors and fallback to empty object
    }
  }
  return {};
};

const readUiChatSelectionModelId = (metadata: Record<string, any>): string | null => {
  const uiChatSelection = metadata?.ui_chat_selection;
  if (!uiChatSelection || typeof uiChatSelection !== 'object' || Array.isArray(uiChatSelection)) {
    return null;
  }
  return normalizeId(uiChatSelection.selected_model_id ?? uiChatSelection.selectedModelId);
};

const readUiChatSelectionAgentId = (metadata: Record<string, any>): string | null => {
  const uiChatSelection = metadata?.ui_chat_selection;
  if (!uiChatSelection || typeof uiChatSelection !== 'object' || Array.isArray(uiChatSelection)) {
    return null;
  }
  return normalizeId(uiChatSelection.selected_agent_id ?? uiChatSelection.selectedAgentId);
};

export const readSessionRuntimeFromMetadata = (
  metadata: any,
): SessionRuntimeMetadata | null => {
  const meta = parseSessionMetadata(metadata);
  const runtime = meta?.chat_runtime;
  const contact = meta?.contact;

  const selectedModelId = normalizeId(
    runtime?.selected_model_id ?? runtime?.selectedModelId,
  ) || readUiChatSelectionModelId(meta);

  const contactAgentId = normalizeId(
    contact?.agent_id ?? meta?.ui_contact?.agent_id,
  ) || readUiChatSelectionAgentId(meta);
  const contactId = normalizeId(
    contact?.contact_id ?? meta?.ui_contact?.contact_id,
  );

  const projectId = normalizeId(
    runtime?.project_id ?? runtime?.projectId,
  );
  const projectRoot = normalizeId(
    runtime?.project_root ?? runtime?.projectRoot,
  );
  const workspaceRoot = normalizeId(
    runtime?.workspace_root ?? runtime?.workspaceRoot,
  );
  const mcpEnabledRaw = runtime?.mcp_enabled ?? runtime?.mcpEnabled;
  const mcpEnabled = typeof mcpEnabledRaw === 'boolean' ? mcpEnabledRaw : true;
  const enabledMcpIds = normalizeIdArray(runtime?.enabled_mcp_ids ?? runtime?.enabledMcpIds);

  if (
    !selectedModelId
    && !contactAgentId
    && !contactId
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
    selectedModelId,
    mcpEnabled,
    enabledMcpIds,
    projectId,
    projectRoot,
    workspaceRoot,
  };
};

export const mergeSessionRuntimeIntoMetadata = (
  metadata: any,
  runtime: Partial<SessionRuntimeMetadata>,
): Record<string, any> => {
  const next = parseSessionMetadata(metadata);
  const existingRuntime = readSessionRuntimeFromMetadata(next);

  const selectedModelId = normalizeId(runtime.selectedModelId ?? existingRuntime?.selectedModelId);
  const contactAgentId = normalizeId(runtime.contactAgentId ?? existingRuntime?.contactAgentId);
  const contactId = normalizeId(runtime.contactId ?? existingRuntime?.contactId);
  const projectId = normalizeId(runtime.projectId ?? existingRuntime?.projectId);
  const projectRoot = normalizeId(runtime.projectRoot ?? existingRuntime?.projectRoot);
  const workspaceRoot = normalizeId(runtime.workspaceRoot ?? existingRuntime?.workspaceRoot);
  const mcpEnabled = typeof runtime.mcpEnabled === 'boolean'
    ? runtime.mcpEnabled
    : (existingRuntime?.mcpEnabled ?? true);
  const enabledMcpIds = runtime.enabledMcpIds
    ? normalizeIdArray(runtime.enabledMcpIds)
    : (existingRuntime?.enabledMcpIds ?? []);

  next.chat_runtime = {
    selected_model_id: selectedModelId,
    mcp_enabled: mcpEnabled,
    enabled_mcp_ids: enabledMcpIds,
    project_id: projectId,
    project_root: projectRoot,
    workspace_root: workspaceRoot,
  };
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
