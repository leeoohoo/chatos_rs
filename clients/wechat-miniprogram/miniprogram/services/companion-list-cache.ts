import type { CompanionAgentWorkspace, CompanionResource } from '../models/api'
import { sessionStore } from '../stores/session-store'
import { agentTeamService } from './agent-team-service'
import { conversationService } from './conversation-service'

const FRESH_MS = 30_000
const STALE_MS = 24 * 60 * 60 * 1_000
const MAX_DEVICE_ENTRIES = 4
const RESOURCE_STORAGE_KEY = 'chatos.companion.resource-cache.v1'
const WORKSPACE_STORAGE_KEY = 'chatos.companion.agent-workspace-cache.v1'

type CacheEntry<T> = {
  deviceId: string
  value: T
  loadedAt: number
}

type StoredCache<T> = {
  ownerUserId: string
  entries: CacheEntry<T>[]
}

let ownerUserId = ''
let resourceEntries = new Map<string, CacheEntry<CompanionResource[]>>()
let workspaceEntries = new Map<string, CacheEntry<CompanionAgentWorkspace>>()
const resourceRequests = new Map<string, Promise<CompanionResource[]>>()
const workspaceRequests = new Map<string, Promise<CompanionAgentWorkspace>>()

function readStored<T>(key: string, currentOwnerUserId: string): Map<string, CacheEntry<T>> {
  if (!currentOwnerUserId) return new Map()
  try {
    const stored = wx.getStorageSync<StoredCache<T>>(key)
    if (!stored || stored.ownerUserId !== currentOwnerUserId || !Array.isArray(stored.entries)) {
      return new Map()
    }
    const now = Date.now()
    return new Map(stored.entries
      .filter((entry) => entry
        && typeof entry.deviceId === 'string'
        && typeof entry.loadedAt === 'number'
        && now - entry.loadedAt <= STALE_MS)
      .sort((left, right) => right.loadedAt - left.loadedAt)
      .slice(0, MAX_DEVICE_ENTRIES)
      .map((entry) => [entry.deviceId, entry]))
  } catch {
    return new Map()
  }
}

function ensureOwner(): string {
  const currentOwnerUserId = sessionStore.user()?.id ?? ''
  if (currentOwnerUserId === ownerUserId) return ownerUserId
  ownerUserId = currentOwnerUserId
  resourceEntries = readStored(RESOURCE_STORAGE_KEY, ownerUserId)
  workspaceEntries = readStored(WORKSPACE_STORAGE_KEY, ownerUserId)
  resourceRequests.clear()
  workspaceRequests.clear()
  return ownerUserId
}

function persist<T>(key: string, entries: Map<string, CacheEntry<T>>): void {
  if (!ownerUserId) return
  const retained = Array.from(entries.values())
    .sort((left, right) => right.loadedAt - left.loadedAt)
    .slice(0, MAX_DEVICE_ENTRIES)
  entries.clear()
  retained.forEach((entry) => entries.set(entry.deviceId, entry))
  try {
    wx.setStorageSync(key, { ownerUserId, entries: retained } satisfies StoredCache<T>)
  } catch {
    // Storage is an optional acceleration. Network loading remains authoritative.
  }
}

function fresh<T>(entry: CacheEntry<T> | undefined): T | undefined {
  if (!entry || Date.now() - entry.loadedAt > FRESH_MS) return undefined
  return entry.value
}

function retainEqualValue<T>(previous: T | undefined, incoming: T): T {
  if (previous === undefined) return incoming
  try {
    return JSON.stringify(previous) === JSON.stringify(incoming) ? previous : incoming
  } catch {
    return incoming
  }
}

export const companionListCache = {
  peekResources(deviceId: string): CompanionResource[] | undefined {
    ensureOwner()
    return resourceEntries.get(deviceId)?.value
  },

  peekWorkspace(deviceId: string): CompanionAgentWorkspace | undefined {
    ensureOwner()
    return workspaceEntries.get(deviceId)?.value
  },

  resources(deviceId: string, force = false): Promise<CompanionResource[]> {
    const requestOwnerUserId = ensureOwner()
    const cached = force ? undefined : fresh(resourceEntries.get(deviceId))
    if (cached) return Promise.resolve(cached)
    const existingRequest = resourceRequests.get(deviceId)
    if (existingRequest) return existingRequest
    const promise = conversationService.resources(deviceId).then((incoming) => {
      if (ensureOwner() !== requestOwnerUserId) return incoming
      const value = retainEqualValue(resourceEntries.get(deviceId)?.value, incoming)
      resourceEntries.set(deviceId, { deviceId, value, loadedAt: Date.now() })
      persist(RESOURCE_STORAGE_KEY, resourceEntries)
      return value
    }).finally(() => {
      if (resourceRequests.get(deviceId) === promise) resourceRequests.delete(deviceId)
    })
    resourceRequests.set(deviceId, promise)
    return promise
  },

  workspace(deviceId: string, force = false): Promise<CompanionAgentWorkspace> {
    const requestOwnerUserId = ensureOwner()
    const cached = force ? undefined : fresh(workspaceEntries.get(deviceId))
    if (cached) return Promise.resolve(cached)
    const existingRequest = workspaceRequests.get(deviceId)
    if (existingRequest) return existingRequest
    const promise = agentTeamService.workspace(deviceId).then((incoming) => {
      if (ensureOwner() !== requestOwnerUserId) return incoming
      const value = retainEqualValue(workspaceEntries.get(deviceId)?.value, incoming)
      workspaceEntries.set(deviceId, { deviceId, value, loadedAt: Date.now() })
      persist(WORKSPACE_STORAGE_KEY, workspaceEntries)
      return value
    }).finally(() => {
      if (workspaceRequests.get(deviceId) === promise) workspaceRequests.delete(deviceId)
    })
    workspaceRequests.set(deviceId, promise)
    return promise
  },

  invalidateResources(deviceId: string): void {
    ensureOwner()
    if (!resourceEntries.delete(deviceId)) return
    persist(RESOURCE_STORAGE_KEY, resourceEntries)
  },

  invalidateWorkspace(deviceId: string): void {
    ensureOwner()
    if (!workspaceEntries.delete(deviceId)) return
    persist(WORKSPACE_STORAGE_KEY, workspaceEntries)
  },

  warm(deviceId: string): void {
    void Promise.allSettled([
      this.resources(deviceId),
      this.workspace(deviceId),
    ])
  },
}
