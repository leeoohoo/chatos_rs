import { useMemo } from 'react';

import type { ChatContact, ChatMessage } from '../types';
import {
  findContactSession,
  formatMessageTime,
  formatRelativeTime,
  mapAgent,
  mapModel,
  value,
  type RawAgent,
  type RawContact,
  type RawModelConfig,
  type RawProjectContact,
  type RawSession,
} from './support';

interface DerivedDataInput {
  persistedMessages: ChatMessage[];
  streamingText: string;
  modelConfigs: RawModelConfig[];
  rawAgents: RawAgent[];
  rawContacts: RawContact[];
  rawSessions: RawSession[];
  rawProjectContacts: RawProjectContact[];
  activeProjectId: string | null;
}

export function useBridgeDerivedData(input: DerivedDataInput) {
  const {
    persistedMessages,
    streamingText,
    modelConfigs,
    rawAgents,
    rawContacts,
    rawSessions,
    rawProjectContacts,
    activeProjectId,
  } = input;
  const messages = useMemo(() => {
    if (!streamingText) return persistedMessages;
    return [
      ...persistedMessages,
      { id: 'live-stream', role: 'assistant' as const, content: streamingText, time: formatMessageTime() },
    ];
  }, [persistedMessages, streamingText]);
  const models = useMemo(() => modelConfigs.filter((item) => item.enabled !== false).map(mapModel), [modelConfigs]);
  const agents = useMemo(() => rawAgents.filter((item) => item.enabled !== false).map(mapAgent), [rawAgents]);
  const accountContacts = useMemo<ChatContact[]>(() => rawContacts.flatMap((contact) => {
    const agentId = value(contact.agent_id, contact.agentId);
    if (!agentId) return [];
    const agent = rawAgents.find((item) => item.id === agentId);
    const session = findContactSession(rawSessions, contact.id, agentId, null);
    return [{
      id: contact.id,
      agentId,
      name: value(contact.agent_name_snapshot, contact.agentNameSnapshot) || agent?.name || '未命名联系人',
      description: agent?.description || null,
      sessionId: session?.id || null,
      projectId: null,
      lastActive: formatRelativeTime(value(session?.updated_at, session?.updatedAt) || value(contact.updated_at, contact.updatedAt) || value(contact.created_at, contact.createdAt)),
    } satisfies ChatContact];
  }), [rawAgents, rawContacts, rawSessions]);
  const contacts = useMemo<ChatContact[]>(() => {
    if (!activeProjectId) return accountContacts;
    return rawProjectContacts.flatMap((link) => {
      const contactId = value(link.contact_id, link.contactId);
      const agentId = value(link.agent_id, link.agentId);
      if (!contactId || !agentId) return [];
      const accountContact = rawContacts.find((item) => item.id === contactId);
      const agent = rawAgents.find((item) => item.id === agentId);
      const preferredSessionId = value(link.latest_session_id, link.latestSessionId);
      const session = rawSessions.find((item) => item.id === preferredSessionId)
        || findContactSession(rawSessions, contactId, agentId, activeProjectId);
      return [{
        id: contactId,
        agentId,
        name: value(link.agent_name_snapshot, link.agentNameSnapshot)
          || value(accountContact?.agent_name_snapshot, accountContact?.agentNameSnapshot)
          || agent?.name
          || '未命名负责人',
        description: agent?.description || null,
        sessionId: session?.id || null,
        projectId: activeProjectId,
        lastActive: formatRelativeTime(value(link.last_message_at, link.lastMessageAt) || value(link.updated_at, link.updatedAt) || value(session?.updated_at, session?.updatedAt)),
      } satisfies ChatContact];
    });
  }, [accountContacts, activeProjectId, rawAgents, rawContacts, rawProjectContacts, rawSessions]);
  const availableAgents = useMemo(() => {
    const existing = new Set(rawContacts.map((item) => value(item.agent_id, item.agentId)).filter(Boolean));
    return agents.filter((agent) => !existing.has(agent.id));
  }, [agents, rawContacts]);

  return { messages, models, agents, accountContacts, contacts, availableAgents };
}
