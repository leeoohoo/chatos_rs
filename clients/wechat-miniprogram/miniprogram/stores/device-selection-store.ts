import type { DeviceSummary } from '../models/api'
import { sessionStore } from './session-store'

const SELECTED_DEVICE_KEY = 'chatos.companion.selected-device.v1'
const SELECTED_DEVICE_SNAPSHOT_KEY = 'chatos.companion.selected-device-snapshot.v1'
const SNAPSHOT_MAX_AGE_MS = 2 * 60 * 1_000
const STALE_SNAPSHOT_MAX_AGE_MS = 24 * 60 * 60 * 1_000

type DeviceSnapshot = {
  device: DeviceSummary
  cachedAt: number
  ownerUserId: string
}

class DeviceSelectionStore {
  get(): string | undefined {
    const value = wx.getStorageSync<string>(SELECTED_DEVICE_KEY)
    return typeof value === 'string' && value.trim() ? value : undefined
  }

  snapshot(maxAgeMs = SNAPSHOT_MAX_AGE_MS): DeviceSummary | undefined {
    const value = wx.getStorageSync<DeviceSnapshot>(SELECTED_DEVICE_SNAPSHOT_KEY)
    const ownerUserId = sessionStore.user()?.id
    if (!ownerUserId
      || !value
      || typeof value !== 'object'
      || !value.device
      || typeof value.cachedAt !== 'number'
      || value.ownerUserId !== ownerUserId) {
      return undefined
    }
    if (Date.now() - value.cachedAt > maxAgeMs || value.device.id !== this.get()) {
      return undefined
    }
    return value.device
  }

  staleSnapshot(): DeviceSummary | undefined {
    return this.snapshot(STALE_SNAPSHOT_MAX_AGE_MS)
  }

  set(deviceId: string, device?: DeviceSummary): void {
    wx.setStorageSync(SELECTED_DEVICE_KEY, deviceId)
    const ownerUserId = sessionStore.user()?.id
    if (device?.id === deviceId && ownerUserId) {
      wx.setStorageSync(SELECTED_DEVICE_SNAPSHOT_KEY, { device, cachedAt: Date.now(), ownerUserId })
    }
  }

  clear(): void {
    wx.removeStorageSync(SELECTED_DEVICE_KEY)
    wx.removeStorageSync(SELECTED_DEVICE_SNAPSHOT_KEY)
  }
}

export const deviceSelectionStore = new DeviceSelectionStore()
