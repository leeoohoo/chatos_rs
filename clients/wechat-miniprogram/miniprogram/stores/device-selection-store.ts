import type { DeviceSummary } from '../models/api'

const SELECTED_DEVICE_KEY = 'chatos.companion.selected-device.v1'
const SELECTED_DEVICE_SNAPSHOT_KEY = 'chatos.companion.selected-device-snapshot.v1'
const SNAPSHOT_MAX_AGE_MS = 2 * 60 * 1_000

type DeviceSnapshot = {
  device: DeviceSummary
  cachedAt: number
}

class DeviceSelectionStore {
  get(): string | undefined {
    const value = wx.getStorageSync<string>(SELECTED_DEVICE_KEY)
    return typeof value === 'string' && value.trim() ? value : undefined
  }

  snapshot(): DeviceSummary | undefined {
    const value = wx.getStorageSync<DeviceSnapshot>(SELECTED_DEVICE_SNAPSHOT_KEY)
    if (!value || typeof value !== 'object' || !value.device || typeof value.cachedAt !== 'number') {
      return undefined
    }
    if (Date.now() - value.cachedAt > SNAPSHOT_MAX_AGE_MS || value.device.id !== this.get()) {
      return undefined
    }
    return value.device
  }

  set(deviceId: string, device?: DeviceSummary): void {
    wx.setStorageSync(SELECTED_DEVICE_KEY, deviceId)
    if (device?.id === deviceId) {
      wx.setStorageSync(SELECTED_DEVICE_SNAPSHOT_KEY, { device, cachedAt: Date.now() })
    }
  }

  clear(): void {
    wx.removeStorageSync(SELECTED_DEVICE_KEY)
    wx.removeStorageSync(SELECTED_DEVICE_SNAPSHOT_KEY)
  }
}

export const deviceSelectionStore = new DeviceSelectionStore()
