import type {
  AskUserPromptListResponse,
  AskUserPromptMutationResponse,
  AskUserPromptRecord,
} from '../models/api'
import { apiRequest } from './api-client'

export const askUserService = {
  async list(conversationId: string): Promise<AskUserPromptRecord[]> {
    const response = await apiRequest<AskUserPromptListResponse>({
      surface: 'chatos',
      path: `/ask-user-prompts?conversation_id=${encodeURIComponent(conversationId)}&include_pending=true&limit=100`,
    })
    return response.prompts
  },

  async submit(
    promptId: string,
    conversationId: string,
    values: Record<string, string>,
    selection?: string | string[],
  ): Promise<AskUserPromptRecord> {
    const response = await apiRequest<AskUserPromptMutationResponse>({
      surface: 'chatos',
      path: `/ask-user-prompts/${encodeURIComponent(promptId)}/submit`,
      method: 'POST',
      data: {
        conversation_id: conversationId,
        values: Object.keys(values).length ? values : undefined,
        selection,
      },
    })
    return response.prompt
  },

  async cancel(promptId: string, conversationId: string): Promise<AskUserPromptRecord> {
    const response = await apiRequest<AskUserPromptMutationResponse>({
      surface: 'chatos',
      path: `/ask-user-prompts/${encodeURIComponent(promptId)}/cancel`,
      method: 'POST',
      data: { conversation_id: conversationId, reason: 'user_cancelled' },
    })
    return response.prompt
  },
}
