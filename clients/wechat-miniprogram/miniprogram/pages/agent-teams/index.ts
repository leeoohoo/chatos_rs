import type {
  CompanionAgentConversationSummary,
  CompanionAgentSummary,
  DeviceSummary,
} from '../../models/api'
import { agentTeamService } from '../../services/agent-team-service'
import { ApiError } from '../../services/api-client'
import { companionListCache } from '../../services/companion-list-cache'
import { deviceService } from '../../services/device-service'
import { deviceSelectionStore } from '../../stores/device-selection-store'
import { sessionStore } from '../../stores/session-store'

type ConversationView = CompanionAgentConversationSummary & {
  initial: string
  kindLabel: string
  preview: string
  updatedLabel: string
}

type AgentView = CompanionAgentSummary & {
  initial: string
  professionLabel: string
  activityLabel: string
}

function relativeUnixTime(value: number): string {
  if (!value) return '暂无消息'
  const seconds = Math.max(0, Math.floor((Date.now() - value) / 1000))
  if (seconds < 60) return '刚刚'
  if (seconds < 3600) return `${Math.floor(seconds / 60)} 分钟前`
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)} 小时前`
  if (seconds < 604_800) return `${Math.floor(seconds / 86_400)} 天前`
  return new Date(value).toLocaleDateString('zh-CN')
}

function conversationView(item: CompanionAgentConversationSummary): ConversationView {
  const last = item.last_message
  const preview = last?.content.trim()
    || last?.attachments[0]?.name
    || (item.goal.trim() || '暂无消息')
  return {
    ...item,
    initial: item.title.trim().slice(0, 1).toUpperCase() || 'A',
    kindLabel: item.kind === 'project_team'
      ? `${item.member_count} 位成员`
      : item.kind === 'human_agent_direct' ? 'Agent 私聊' : '协作私聊',
    preview,
    updatedLabel: relativeUnixTime(item.updated_at_unix_ms),
  }
}

function agentView(item: CompanionAgentSummary): AgentView {
  return {
    ...item,
    initial: item.name.trim().slice(0, 1).toUpperCase() || 'A',
    professionLabel: item.profession_key.replace(/_/g, ' '),
    activityLabel: item.heartbeat_enabled
      ? `定时工作已开启${item.last_heartbeat_at_unix_ms ? ` · ${relativeUnixTime(item.last_heartbeat_at_unix_ms)}` : ''}`
      : '按需唤醒',
  }
}

function workspaceViews(workspace: {
  teams: CompanionAgentConversationSummary[]
  direct_conversations: CompanionAgentConversationSummary[]
  agents: CompanionAgentSummary[]
}) {
  return {
    teams: workspace.teams.map(conversationView),
    directs: workspace.direct_conversations.map(conversationView),
    agents: workspace.agents.map(agentView),
  }
}

Page({
  data: {
    section: 'conversations' as 'conversations' | 'agents',
    teams: [] as ConversationView[],
    directs: [] as ConversationView[],
    agents: [] as AgentView[],
    device: undefined as DeviceSummary | undefined,
    loading: true,
    error: '',
    openingId: '',
  },

  pageVisible: false,
  refreshTimer: undefined as ReturnType<typeof setInterval> | undefined,
  retryTimer: undefined as ReturnType<typeof setTimeout> | undefined,
  requestInFlight: false,
  retryAttempt: 0,

  onShow() {
    this.pageVisible = true
    const cachedDevice = deviceSelectionStore.snapshot()
    if (cachedDevice) {
      const cachedWorkspace = companionListCache.peekWorkspace(cachedDevice.id)
      this.setData({
        device: cachedDevice,
        ...(cachedWorkspace ? { ...workspaceViews(cachedWorkspace), loading: false } : {}),
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
    void this.loadWorkspace()
    this.startRefresh()
  },

  onHide() {
    this.pageVisible = false
    this.stopRefresh()
    this.stopRetry()
  },

  onUnload() {
    this.pageVisible = false
    this.stopRefresh()
    this.stopRetry()
  },

  onPullDownRefresh() {
    void this.loadWorkspace(true).finally(() => wx.stopPullDownRefresh())
  },

  switchSection(event: WechatMiniprogram.TouchEvent) {
    const section = String(event.currentTarget.dataset.section ?? '')
    if (section === 'conversations' || section === 'agents') this.setData({ section })
  },

  async selectedDevice(): Promise<DeviceSummary | undefined> {
    const devices = await deviceService.list()
    const selectedId = deviceSelectionStore.get()
    const selected = devices.find((device) => device.id === selectedId)
      ?? devices.find((device) => device.is_online)
      ?? devices[0]
    const cached = deviceSelectionStore.snapshot()
    if (selected) {
      // Presence can briefly disappear while the desktop connector is replacing its
      // WebSocket. Keep the last online snapshot long enough to retry the relay call
      // instead of immediately emptying the whole Agent workspace.
      if (!selected.is_online && cached?.id === selected.id && cached.is_online) return cached
      deviceSelectionStore.set(selected.id, selected)
    }
    return selected
  },

  async loadWorkspace(force = false) {
    if (this.requestInFlight) return
    this.requestInFlight = true
    const hasContent = this.data.teams.length + this.data.directs.length + this.data.agents.length > 0
    this.setData({ loading: !hasContent, error: '' })
    try {
      const cachedDevice = deviceSelectionStore.snapshot()
      const device = cachedDevice ?? await this.selectedDevice()
      const refreshedDevice = cachedDevice ? this.selectedDevice().catch(() => undefined) : undefined
      if (!this.pageVisible) return
      if (!device) {
        this.setData({ device: undefined, teams: [], directs: [], agents: [], loading: false })
        return
      }
      if (!device.is_online) {
        this.handleTransientRefresh(device, hasContent, '所选电脑正在重新连接')
        return
      }
      const workspace = await companionListCache.workspace(device.id, force)
      if (!this.pageVisible) return
      this.retryAttempt = 0
      this.stopRetry()
      this.setData({
        device,
        ...workspaceViews(workspace),
        loading: false,
      })
      if (refreshedDevice) {
        void refreshedDevice.then((latest) => {
          if (!this.pageVisible || !latest) return
          if (latest.id !== device.id) void this.loadWorkspace(true)
          else this.setData({ device: latest })
        })
      }
    } catch (error) {
      if (!this.pageVisible) return
      if (this.isTransientRefreshError(error)) {
        this.handleTransientRefresh(this.data.device, hasContent, '正在重新连接电脑')
        return
      }
      this.setData({ loading: false, error: error instanceof Error ? error.message : '读取 Agent 团队失败' })
    } finally {
      this.requestInFlight = false
    }
  },

  isTransientRefreshError(error: unknown): boolean {
    return error instanceof ApiError
      && (error.statusCode === 0 || error.statusCode === 502 || error.statusCode === 503 || error.statusCode === 504)
  },

  handleTransientRefresh(device: DeviceSummary | undefined, hasContent: boolean, message: string) {
    this.setData({
      device: device ?? this.data.device,
      loading: !hasContent,
      // Existing data is still valid during a connector handover. Do not replace it
      // with a red error banner for a single failed background refresh.
      error: hasContent ? '' : `${message}…`,
    })
    this.scheduleRetry()
  },

  scheduleRetry() {
    if (!this.pageVisible || this.retryTimer || this.retryAttempt >= 4) return
    const delay = Math.min(4_000, 750 * (2 ** this.retryAttempt))
    this.retryAttempt += 1
    this.retryTimer = setTimeout(() => {
      this.retryTimer = undefined
      if (this.pageVisible) void this.loadWorkspace()
    }, delay)
  },

  stopRetry() {
    if (this.retryTimer) clearTimeout(this.retryTimer)
    this.retryTimer = undefined
  },

  openConversation(event: WechatMiniprogram.TouchEvent) {
    const id = String(event.currentTarget.dataset.id ?? '')
    if (!id) return
    wx.navigateTo({ url: `/pages/agent-conversation-detail/index?id=${encodeURIComponent(id)}` })
  },

  async openAgent(event: WechatMiniprogram.TouchEvent) {
    const agentId = String(event.currentTarget.dataset.id ?? '')
    const deviceId = this.data.device?.id
    if (!agentId || !deviceId || this.data.openingId) return
    this.setData({ openingId: agentId, error: '' })
    try {
      const detail = await agentTeamService.openDirect(deviceId, agentId)
      wx.navigateTo({
        url: `/pages/agent-conversation-detail/index?id=${encodeURIComponent(detail.conversation.id)}`,
      })
    } catch (error) {
      this.setData({ error: error instanceof Error ? error.message : '打开 Agent 私聊失败' })
    } finally {
      this.setData({ openingId: '' })
    }
  },

  startRefresh() {
    this.stopRefresh()
    this.refreshTimer = setInterval(() => {
      if (this.pageVisible && !this.data.loading && !this.data.openingId) void this.loadWorkspace()
    }, 30_000)
  },

  stopRefresh() {
    if (this.refreshTimer) clearInterval(this.refreshTimer)
    this.refreshTimer = undefined
  },
})
