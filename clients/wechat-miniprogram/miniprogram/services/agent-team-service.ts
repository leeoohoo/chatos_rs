import type {
  CompanionAgentConversationDetail,
  CompanionAgentMessagePage,
  CompanionAgentSendResponse,
  CompanionAgentWorkspace,
} from '../models/api'
import { apiRequest } from './api-client'

export function createAgentClientMessageId(): string {
  const bytes = new Uint8Array(16)
  for (let index = 0; index < bytes.length; index += 1) bytes[index] = Math.floor(Math.random() * 256)
  bytes[6] = (bytes[6] & 0x0f) | 0x40
  bytes[8] = (bytes[8] & 0x3f) | 0x80
  const hex = Array.from(bytes, (value) => value.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

function base(deviceId: string): string {
  return `/companion/devices/${encodeURIComponent(deviceId)}`
}

export const agentTeamService = {
  workspace(deviceId: string): Promise<CompanionAgentWorkspace> {
    return apiRequest({ surface: 'local', path: `${base(deviceId)}/agent-workspace` })
  },

  conversation(deviceId: string, conversationId: string): Promise<CompanionAgentConversationDetail> {
    return apiRequest({
      surface: 'local',
      path: `${base(deviceId)}/agent-conversations/${encodeURIComponent(conversationId)}`,
    })
  },

  messages(
    deviceId: string,
    conversationId: string,
    options: { before?: string; after?: string; limit?: number } = {},
  ): Promise<CompanionAgentMessagePage> {
    const params: string[] = [`limit=${options.limit ?? 40}`]
    if (options.before) params.push(`before_message_id=${encodeURIComponent(options.before)}`)
    if (options.after) params.push(`after_message_id=${encodeURIComponent(options.after)}`)
    return apiRequest({
      surface: 'local',
      path: `${base(deviceId)}/agent-conversations/${encodeURIComponent(conversationId)}/messages?${params.join('&')}`,
    })
  },

  send(
    deviceId: string,
    conversationId: string,
    content: string,
    mentionedAgentIds: string[],
    clientMessageId = createAgentClientMessageId(),
  ): Promise<CompanionAgentSendResponse> {
    return apiRequest({
      surface: 'local',
      path: `${base(deviceId)}/agent-conversations/${encodeURIComponent(conversationId)}/messages`,
      method: 'POST',
      data: {
        content,
        mentioned_agent_ids: mentionedAgentIds,
        client_message_id: clientMessageId,
      },
    })
  },

  openDirect(deviceId: string, agentId: string): Promise<CompanionAgentConversationDetail> {
    return apiRequest({
      surface: 'local',
      path: `${base(deviceId)}/agents/${encodeURIComponent(agentId)}/direct-conversation`,
      method: 'POST',
      data: {},
    })
  },
}
