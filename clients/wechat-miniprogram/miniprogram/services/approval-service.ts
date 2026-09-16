import type { CompanionApproval, CompanionApprovalResolution } from '../models/api'
import { apiRequest } from './api-client'

export const approvalService = {
  list(deviceId: string): Promise<CompanionApproval[]> {
    return apiRequest({
      surface: 'local',
      path: `/companion/devices/${encodeURIComponent(deviceId)}/approvals`,
    })
  },

  resolve(
    deviceId: string,
    approvalId: string,
    decision: 'accept' | 'acceptForSession' | 'decline',
  ): Promise<CompanionApprovalResolution> {
    return apiRequest({
      surface: 'local',
      path: `/companion/devices/${encodeURIComponent(deviceId)}/approvals/${encodeURIComponent(approvalId)}/resolve`,
      method: 'POST',
      data: { decision },
    })
  },
}
