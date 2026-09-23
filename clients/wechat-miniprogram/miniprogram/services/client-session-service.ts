import type { ClientSession } from '../models/api'
import { apiRequest } from './api-client'

export const clientSessionService = {
  list(): Promise<ClientSession[]> {
    return apiRequest({ surface: 'user', path: '/auth/client-sessions' })
  },

  revoke(id: string): Promise<void> {
    return apiRequest({
      surface: 'user',
      path: `/auth/client-sessions/${encodeURIComponent(id)}`,
      method: 'DELETE',
    })
  },
}
