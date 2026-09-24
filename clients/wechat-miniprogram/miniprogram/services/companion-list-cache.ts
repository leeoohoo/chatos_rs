import type { CompanionAgentWorkspace, CompanionResource } from '../models/api'
import { agentTeamService } from './agent-team-service'
import { conversationService } from './conversation-service'

const FRESH_MS = 30_000

type CacheEntry<T> = {
  deviceId: string
  value: T
  loadedAt: number
}

let resourceEntry: CacheEntry<CompanionResource[]> | undefined
let workspaceEntry: CacheEntry<CompanionAgentWorkspace> | undefined
let resourceRequest: { deviceId: string; promise: Promise<CompanionResource[]> } | undefined
let workspaceRequest: { deviceId: string; promise: Promise<CompanionAgentWorkspace> } | undefined

function fresh<T>(entry: CacheEntry<T> | undefined, deviceId: string): T | undefined {
  if (!entry || entry.deviceId !== deviceId || Date.now() - entry.loadedAt > FRESH_MS) return undefined
  return entry.value
}

export const companionListCache = {
  peekResources(deviceId: string): CompanionResource[] | undefined {
    return resourceEntry?.deviceId === deviceId ? resourceEntry.value : undefined
  },

  peekWorkspace(deviceId: string): CompanionAgentWorkspace | undefined {
    return workspaceEntry?.deviceId === deviceId ? workspaceEntry.value : undefined
  },

  resources(deviceId: string, force = false): Promise<CompanionResource[]> {
    const cached = force ? undefined : fresh(resourceEntry, deviceId)
    if (cached) return Promise.resolve(cached)
    if (resourceRequest?.deviceId === deviceId) return resourceRequest.promise
    const promise = conversationService.resources(deviceId).then((value) => {
      resourceEntry = { deviceId, value, loadedAt: Date.now() }
      return value
    }).finally(() => {
      if (resourceRequest?.promise === promise) resourceRequest = undefined
    })
    resourceRequest = { deviceId, promise }
    return promise
  },

  workspace(deviceId: string, force = false): Promise<CompanionAgentWorkspace> {
    const cached = force ? undefined : fresh(workspaceEntry, deviceId)
    if (cached) return Promise.resolve(cached)
    if (workspaceRequest?.deviceId === deviceId) return workspaceRequest.promise
    const promise = agentTeamService.workspace(deviceId).then((value) => {
      workspaceEntry = { deviceId, value, loadedAt: Date.now() }
      return value
    }).finally(() => {
      if (workspaceRequest?.promise === promise) workspaceRequest = undefined
    })
    workspaceRequest = { deviceId, promise }
    return promise
  },

  warm(deviceId: string): void {
    void Promise.allSettled([
      this.resources(deviceId),
      this.workspace(deviceId),
    ])
  },
}
