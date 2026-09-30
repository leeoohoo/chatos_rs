import type {
  AskUserPromptListResponse,
  AskUserPromptMutationResponse,
  AskUserPromptRecord,
} from '../models/api'
import { apiRequest } from './api-client'

export const askUserService = {
  async list(deviceId: string, conversationId: string): Promise<AskUserPromptRecord[]> {
    const response = await apiRequest<AskUserPromptListResponse>({
      surface: 'local',
      path: `${base(deviceId, conversationId)}/ask-user-prompts`,
    })
    return response.prompts
  },

  async submit(
    promptId: string,
    deviceId: string,
    conversationId: string,
    values: Record<string, string>,
    selection?: string | string[],
  ): Promise<AskUserPromptRecord> {
    const response = await apiRequest<AskUserPromptMutationResponse>({
      surface: 'local',
      path: `${base(deviceId, conversationId)}/ask-user-prompts/${encodeURIComponent(promptId)}/submit`,
      method: 'POST',
      data: {
        values: Object.keys(values).length ? values : undefined,
        selection,
      },
    })
    return response.prompt
  },

  async cancel(promptId: string, deviceId: string, conversationId: string): Promise<AskUserPromptRecord> {
    const response = await apiRequest<AskUserPromptMutationResponse>({
      surface: 'local',
      path: `${base(deviceId, conversationId)}/ask-user-prompts/${encodeURIComponent(promptId)}/cancel`,
      method: 'POST',
      data: {},
    })
    return response.prompt
  },
}

function base(deviceId: string, conversationId: string): string {
  return `/companion/devices/${encodeURIComponent(deviceId)}/conversations/${encodeURIComponent(conversationId)}`
}
