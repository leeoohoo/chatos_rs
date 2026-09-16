import type {
  CompactHistoryPage,
  CompanionTaskListResponse,
  CompanionResource,
  Conversation,
  ConversationRuntimeContext,
  GuidanceResponse,
  SendMessageResponse,
  WsTicketResponse,
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

  get(id: string): Promise<Conversation> {
    return apiRequest({ surface: 'chatos', path: `/companion/conversations/${encodeURIComponent(id)}` })
  },

  history(id: string, before?: string): Promise<CompactHistoryPage> {
    const query = before ? `?limit=20&before=${encodeURIComponent(before)}` : '?limit=10'
    return apiRequest({
      surface: 'chatos',
      path: `/companion/conversations/${encodeURIComponent(id)}/compact-history${query}`,
    })
  },

  runtimeContext(id: string): Promise<ConversationRuntimeContext | null> {
    return apiRequest({
      surface: 'chatos',
      path: `/companion/conversations/${encodeURIComponent(id)}/state`,
    })
  },

  tasks(messageId: string, taskId?: string): Promise<CompanionTaskListResponse> {
    const query = taskId ? `?task_id=${encodeURIComponent(taskId)}` : ''
    return apiRequest({
      surface: 'chatos',
      path: `/companion/messages/${encodeURIComponent(messageId)}/tasks${query}`,
    })
  },

  send(id: string, content: string): Promise<SendMessageResponse> {
    const turnId = uuid()
    return apiRequest({
      surface: 'chatos',
      path: '/agent/chat/send',
      method: 'POST',
      headers: { 'Idempotency-Key': turnId },
      data: { conversation_id: id, content, turn_id: turnId },
    })
  },

  guidance(id: string, turnId: string, content: string): Promise<GuidanceResponse> {
    return apiRequest({
      surface: 'chatos',
      path: '/agent/chat/guidance',
      method: 'POST',
      data: { conversation_id: id, turn_id: turnId, content },
    })
  },

  stop(id: string, turnId?: string): Promise<{ success: boolean }> {
    return apiRequest({
      surface: 'chatos',
      path: '/agent/chat/stop',
      method: 'POST',
      data: { conversation_id: id, turn_id: turnId },
    })
  },

  websocketTicket(): Promise<WsTicketResponse> {
    return apiRequest({ surface: 'chatos', path: '/auth/ws-ticket', method: 'POST' })
  },
}
