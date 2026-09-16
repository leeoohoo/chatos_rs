import type { DeviceSummary } from '../../models/api'
import { deviceService } from '../../services/device-service'
import { sessionStore } from '../../stores/session-store'
import { deviceSelectionStore } from '../../stores/device-selection-store'
import { relativeTime } from '../../utils/presentation'

type DeviceView = DeviceSummary & { lastSeenLabel: string }

Page({
  data: {
    devices: [] as DeviceView[],
    loading: true,
    error: '',
    onlineCount: 0,
    openingDeviceId: '',
  },

  refreshTimer: undefined as ReturnType<typeof setInterval> | undefined,
  requestInFlight: false,
  pageVisible: false,

  async onShow() {
    this.pageVisible = true
    await getApp<IAppOption>().authReady
    if (!this.pageVisible) return
    if (!sessionStore.hasToken()) {
      wx.reLaunch({ url: '/pages/bind/index' })
      return
    }
    void this.loadDevices()
    this.stopRefreshTimer()
    this.refreshTimer = setInterval(() => {
      if (!this.pageVisible) {
        this.stopRefreshTimer()
        return
      }
      void this.loadDevices(false)
    }, 15_000)
  },

  onHide() {
    this.pageVisible = false
    this.stopRefreshTimer()
  },

  onUnload() {
    this.pageVisible = false
    this.stopRefreshTimer()
  },

  onPullDownRefresh() {
    void this.loadDevices().finally(() => wx.stopPullDownRefresh())
  },

  async loadDevices(showErrors = true) {
    if (this.requestInFlight) return
    this.requestInFlight = true
    this.setData({ loading: this.data.devices.length === 0, error: showErrors ? '' : this.data.error })
    try {
      const devices = (await deviceService.list()).map((device) => ({
        ...device,
        lastSeenLabel: relativeTime(device.last_seen_at),
      }))
      const selectedDeviceId = deviceSelectionStore.get()
      const selectedDevice = devices.find((device) => device.id === selectedDeviceId)
      if (selectedDevice) {
        deviceSelectionStore.set(selectedDevice.id, selectedDevice)
      } else {
        const preferred = devices.find((device) => device.is_online) ?? devices[0]
        if (preferred) deviceSelectionStore.set(preferred.id, preferred)
        else deviceSelectionStore.clear()
      }
      this.setData({
        devices,
        loading: false,
        onlineCount: devices.filter((device) => device.is_online).length,
      })
    } catch (error) {
      this.setData({
        loading: false,
        error: showErrors ? (error instanceof Error ? error.message : '加载失败') : this.data.error,
      })
    } finally {
      this.requestInFlight = false
    }
  },

  openDevice(event: WechatMiniprogram.TouchEvent) {
    const id = String(event.currentTarget.dataset.id ?? '')
    const device = this.data.devices.find((item) => item.id === id)
    if (!device || this.data.openingDeviceId) return
    deviceSelectionStore.set(id, device)
    this.setData({ openingDeviceId: id, error: '' })
    wx.switchTab({
      url: '/pages/conversations/index',
      success: () => this.setData({ openingDeviceId: '' }),
      fail: () => {
        wx.reLaunch({
          url: '/pages/conversations/index',
          complete: () => this.setData({ openingDeviceId: '' }),
        })
      },
    })
  },

  stopRefreshTimer() {
    if (this.refreshTimer) clearInterval(this.refreshTimer)
    this.refreshTimer = undefined
  },
})
