import type { DeviceSummary } from '../models/api'
import { apiRequest } from './api-client'

export const deviceService = {
  list(): Promise<DeviceSummary[]> {
    return apiRequest({ surface: 'local', path: '/companion/devices' })
  },
}
