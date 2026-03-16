import type { Session } from '../../../types';
import type ApiClient from '../../api/client';

export const normalizeSession = (raw: any): Session => {
  const status = typeof raw?.status === 'string'
    ? raw.status.toLowerCase()
    : (raw?.archived ? 'archived' : 'active');
  const archived = raw?.archived === true || status === 'archiving' || status === 'archived';
  const metadataFromRaw = (raw?.metadata && typeof raw.metadata === 'object' && !Array.isArray(raw.metadata))
    ? raw.metadata
    : {};
  const selectedModelId = typeof raw?.selected_model_id === 'string'
    ? raw.selected_model_id.trim()
    : (typeof metadataFromRaw?.chat_runtime?.selected_model_id === 'string'
      ? metadataFromRaw.chat_runtime.selected_model_id.trim()
      : '');
  const selectedAgentId = typeof raw?.selected_agent_id === 'string'
    ? raw.selected_agent_id.trim()
    : (typeof metadataFromRaw?.contact?.agent_id === 'string'
      ? metadataFromRaw.contact.agent_id.trim()
      : '');
  let metadata = raw?.metadata ?? null;
  const hasSelection = selectedModelId.length > 0 || selectedAgentId.length > 0;
  if (hasSelection) {
    const metadataObject = (metadata && typeof metadata === 'object' && !Array.isArray(metadata))
      ? { ...metadata }
      : {};
    metadataObject.ui_chat_selection = {
      selected_model_id: selectedModelId.length > 0 ? selectedModelId : null,
      selected_agent_id: selectedAgentId.length > 0 ? selectedAgentId : null,
    };
    metadataObject.chat_runtime = {
      ...(metadataObject.chat_runtime && typeof metadataObject.chat_runtime === 'object' && !Array.isArray(metadataObject.chat_runtime)
        ? metadataObject.chat_runtime
        : {}),
      selected_model_id: selectedModelId.length > 0 ? selectedModelId : null,
    };
    metadataObject.contact = {
      ...(metadataObject.contact && typeof metadataObject.contact === 'object' && !Array.isArray(metadataObject.contact)
        ? metadataObject.contact
        : {}),
      type: 'memory_agent',
      agent_id: selectedAgentId.length > 0 ? selectedAgentId : null,
    };
    metadata = metadataObject;
  }

  return {
    id: raw?.id,
    title: raw?.title ?? '',
    createdAt: new Date(raw?.created_at ?? raw?.createdAt ?? Date.now()),
    updatedAt: new Date(raw?.updated_at ?? raw?.updatedAt ?? raw?.created_at ?? raw?.createdAt ?? Date.now()),
    messageCount: raw?.messageCount ?? raw?.message_count ?? 0,
    tokenUsage: raw?.tokenUsage ?? raw?.token_usage ?? 0,
    pinned: raw?.pinned ?? false,
    archived,
    status,
    tags: raw?.tags ?? null,
    metadata,
  };
};

export const fetchSession = async (client: ApiClient, sessionId: string): Promise<Session | null> => {
  try {
    const session = await client.getSession(sessionId);
    if (!session) return null;
    return normalizeSession(session);
  } catch {
    return null;
  }
};
