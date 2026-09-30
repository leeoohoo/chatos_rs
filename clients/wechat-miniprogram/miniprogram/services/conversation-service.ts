import type {
  CompactHistoryPage,
  CompanionTaskListResponse,
  CompanionResource,
  Conversation,
  ConversationRuntimeContext,
  GuidanceResponse,
  SendMessageResponse,
} from '../models/api'
import { apiRequest } from './api-client'

function uuid(): string {
  const bytes = new Uint8Array(16)
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Math.floor(Math.random() * 256)
  }
  bytes[6] = (bytes[6] & 0x0f) | 0x40
  bytes[8] = (bytes[8] & 0x3f) | 0x80
  const hex = Array.from(bytes, (value) => value.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

export const conversationService = {
  resources(deviceId: string): Promise<CompanionResource[]> {
    return apiRequest({
      surface: 'local',
      path: `/companion/devices/${encodeURIComponent(deviceId)}/resources`,
    })
  },

  resolveResource(deviceId: string, resourceId: string): Promise<CompanionResource> {
    return apiRequest({
      surface: 'local',
      path: `/companion/devices/${encodeURIComponent(deviceId)}/resources/resolve`,
      method: 'POST',
      data: { resource_id: resourceId },
    })
  },

  get(deviceId: string, id: string): Promise<Conversation> {
    return apiRequest({ surface: 'local', path: `${base(deviceId)}/conversations/${encodeURIComponent(id)}` })
  },

  history(deviceId: string, id: string, before?: string): Promise<CompactHistoryPage> {
    const query = before ? `?limit=20&before=${encodeURIComponent(before)}` : '?limit=10'
    return apiRequest({
      surface: 'local',
      path: `${base(deviceId)}/conversations/${encodeURIComponent(id)}/history${query}`,
    })
  },

  runtimeContext(deviceId: string, id: string): Promise<ConversationRuntimeContext | null> {
    return apiRequest({
      surface: 'local',
      path: `${base(deviceId)}/conversations/${encodeURIComponent(id)}/state`,
    })
  },

  tasks(
    deviceId: string,
    conversationId: string,
    messageId: string,
    taskId?: string,
  ): Promise<CompanionTaskListResponse> {
    const query = taskId ? `?task_id=${encodeURIComponent(taskId)}` : ''
    return apiRequest({
      surface: 'local',
      path: `${base(deviceId)}/conversations/${encodeURIComponent(conversationId)}/messages/${encodeURIComponent(messageId)}/tasks${query}`,
      clearSessionOnUnauthorized: false,
    })
  },

  send(deviceId: string, id: string, content: string): Promise<SendMessageResponse> {
    const turnId = uuid()
    return apiRequest({
      surface: 'local',
      path: `${base(deviceId)}/conversations/${encodeURIComponent(id)}/messages`,
      method: 'POST',
      data: { content, turn_id: turnId },
    })
  },

  guidance(deviceId: string, id: string, turnId: string, content: string): Promise<GuidanceResponse> {
    return apiRequest({
      surface: 'local',
      path: `${base(deviceId)}/conversations/${encodeURIComponent(id)}/guidance`,
      method: 'POST',
      data: { turn_id: turnId, content },
    })
  },

  stop(deviceId: string, id: string, turnId?: string): Promise<{ success: boolean }> {
    return apiRequest({
      surface: 'local',
      path: `${base(deviceId)}/conversations/${encodeURIComponent(id)}/stop`,
      method: 'POST',
      data: { turn_id: turnId },
    })
  },
}

function base(deviceId: string): string {
  return `/companion/devices/${encodeURIComponent(deviceId)}`
}
