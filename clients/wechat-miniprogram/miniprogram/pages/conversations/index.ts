import type { CompanionResource, DeviceSummary } from '../../models/api'
import { companionListCache } from '../../services/companion-list-cache'
import { conversationService } from '../../services/conversation-service'
import { deviceService } from '../../services/device-service'
import { finishTabSwitch } from '../../services/tab-navigation'
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

function sameDevice(left: DeviceSummary | undefined, right: DeviceSummary | undefined): boolean {
  if (!left || !right) return left === right
  return left.id === right.id
    && left.display_name === right.display_name
    && left.is_online === right.is_online
    && left.status === right.status
    && left.updated_at === right.updated_at
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
  renderedResources: undefined as CompanionResource[] | undefined,

  onShow() {
    this.pageVisible = true
    finishTabSwitch(this, 1)
    if (!sessionStore.hasToken()) {
      void this.activate()
      return
    }
    const cachedDevice = deviceSelectionStore.staleSnapshot()
    if (cachedDevice) {
      const cachedResources = companionListCache.peekResources(cachedDevice.id)
      const deviceChanged = !sameDevice(this.data.device, cachedDevice)
      const resourcesChanged = Boolean(cachedResources && cachedResources !== this.renderedResources)
      if (deviceChanged || resourcesChanged || (cachedResources && this.data.loading)) {
        if (cachedResources) this.renderedResources = cachedResources
        else if (this.data.device?.id !== cachedDevice.id) this.renderedResources = undefined
        this.setData({
          ...(deviceChanged ? { device: cachedDevice } : {}),
          ...(resourcesChanged ? { resources: resourceViews(cachedResources!) } : {}),
          ...(deviceChanged && !cachedResources ? { resources: [], loading: true } : {}),
          ...(cachedResources && this.data.loading ? { loading: false } : {}),
        })
      }
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
    const loading = this.data.resources.length === 0
    if (this.data.loading !== loading || this.data.error) this.setData({ loading, error: '' })
    try {
      const cachedDevice = deviceSelectionStore.snapshot()
      const device = cachedDevice ?? await this.selectedDevice()
      const refreshedDevice = cachedDevice ? this.selectedDevice().catch(() => undefined) : undefined
      if (!device) {
        this.renderedResources = undefined
        this.setData({ resources: [], device: undefined, loading: false })
        return
      }
      if (!device.is_online) {
        this.renderedResources = undefined
        this.setData({ resources: [], device, loading: false, error: '所选电脑当前离线' })
        return
      }
      const rawResources = await companionListCache.resources(device.id, force)
      if (!this.pageVisible) return
      const resourcesChanged = rawResources !== this.renderedResources
      if (resourcesChanged) this.renderedResources = rawResources
      if (resourcesChanged || !sameDevice(this.data.device, device) || this.data.loading || this.data.error) {
        this.setData({
          ...(resourcesChanged ? { resources: resourceViews(rawResources) } : {}),
          ...(!sameDevice(this.data.device, device) ? { device } : {}),
          ...(this.data.loading ? { loading: false } : {}),
          ...(this.data.error ? { error: '' } : {}),
        })
      }
      if (refreshedDevice) {
        void refreshedDevice.then((latest) => {
          if (!this.pageVisible || !latest) return
          if (latest.id !== device.id) void this.loadResources(true)
          else if (!sameDevice(this.data.device, latest)) this.setData({ device: latest })
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
