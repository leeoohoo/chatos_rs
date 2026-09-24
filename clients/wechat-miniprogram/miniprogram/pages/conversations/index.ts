import type { CompanionResource, DeviceSummary } from '../../models/api'
import { companionListCache } from '../../services/companion-list-cache'
import { conversationService } from '../../services/conversation-service'
import { deviceService } from '../../services/device-service'
import { deviceSelectionStore } from '../../stores/device-selection-store'
import { sessionStore } from '../../stores/session-store'
import { relativeTime } from '../../utils/presentation'

type ResourceView = CompanionResource & {
  updatedLabel: string
  messageLabel: string
  initial: string
  kindLabel: string
}

function resourceViews(rawResources: CompanionResource[]): ResourceView[] {
  return rawResources.map((resource) => ({
    ...resource,
    updatedLabel: resource.updated_at ? relativeTime(resource.updated_at) : '尚未开始',
    messageLabel: resource.message_count > 0 ? `${resource.message_count} 条消息` : '暂无消息',
    initial: resource.title.trim().slice(0, 1).toUpperCase() || 'C',
    kindLabel: resource.kind === 'contact' ? '联系人' : '项目',
  }))
}

Page({
  data: {
    resources: [] as ResourceView[],
    device: undefined as DeviceSummary | undefined,
    loading: true,
    error: '',
    openingId: '',
  },

  pageVisible: false,
  requestInFlight: false,

  onShow() {
    this.pageVisible = true
    const cachedDevice = deviceSelectionStore.snapshot()
    if (cachedDevice) {
      const cachedResources = companionListCache.peekResources(cachedDevice.id)
      this.setData({
        device: cachedDevice,
        ...(cachedResources ? { resources: resourceViews(cachedResources), loading: false } : {}),
      })
    }
    void this.activate()
  },

  async activate() {
    await getApp<IAppOption>().authReady
    if (!this.pageVisible) return
    if (!sessionStore.hasToken()) {
      wx.reLaunch({ url: '/pages/bind/index' })
      return
    }
    void this.loadResources()
  },

  onHide() {
    this.pageVisible = false
  },

  onUnload() {
    this.pageVisible = false
  },

  onPullDownRefresh() {
    void this.loadResources(true).finally(() => wx.stopPullDownRefresh())
  },

  async selectedDevice(): Promise<DeviceSummary | undefined> {
    const devices = await deviceService.list()
    const selectedId = deviceSelectionStore.get()
    const selected = devices.find((device) => device.id === selectedId)
      ?? devices.find((device) => device.is_online)
      ?? devices[0]
    if (selected) deviceSelectionStore.set(selected.id, selected)
    return selected
  },

  async loadResources(force = false) {
    if (this.requestInFlight) return
    this.requestInFlight = true
    this.setData({ loading: this.data.resources.length === 0, error: '' })
    try {
      const cachedDevice = deviceSelectionStore.snapshot()
      const device = cachedDevice ?? await this.selectedDevice()
      const refreshedDevice = cachedDevice ? this.selectedDevice().catch(() => undefined) : undefined
      if (!device) {
        this.setData({ resources: [], device: undefined, loading: false })
        return
      }
      if (!device.is_online) {
        this.setData({ resources: [], device, loading: false, error: '所选电脑当前离线' })
        return
      }
      const rawResources = await companionListCache.resources(device.id, force)
      if (!this.pageVisible) return
      const resources = resourceViews(rawResources)
      this.setData({ resources, device, loading: false })
      if (refreshedDevice) {
        void refreshedDevice.then((latest) => {
          if (!this.pageVisible || !latest) return
          if (latest.id !== device.id) void this.loadResources(true)
          else this.setData({ device: latest })
        })
      }
    } catch (error) {
      this.setData({
        loading: false,
        error: error instanceof Error ? error.message : '读取电脑会话失败',
      })
    } finally {
      this.requestInFlight = false
    }
  },

  async openResource(event: WechatMiniprogram.TouchEvent) {
    const resourceId = String(event.currentTarget.dataset.id ?? '')
    const deviceId = this.data.device?.id
    if (!resourceId || !deviceId || this.data.openingId) return
    const current = this.data.resources.find((item) => item.id === resourceId)
    if (!current) return
    this.setData({ openingId: resourceId, error: '' })
    try {
      const resolved = current.conversation_id
        ? current
        : await conversationService.resolveResource(deviceId, resourceId)
      if (!resolved.conversation_id) throw new Error('桌面客户端还没有为此入口准备会话')
      wx.navigateTo({
        url: `/pages/conversation-detail/index?id=${encodeURIComponent(resolved.conversation_id)}`,
      })
    } catch (error) {
      this.setData({ error: error instanceof Error ? error.message : '打开会话失败' })
    } finally {
      this.setData({ openingId: '' })
    }
  },
})
