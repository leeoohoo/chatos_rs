import type { ClientSession } from '../../models/api'
import { authService } from '../../services/auth-service'
import { clientSessionService } from '../../services/client-session-service'
import { finishTabSwitch } from '../../services/tab-navigation'
import { sessionStore } from '../../stores/session-store'
import { relativeTime } from '../../utils/presentation'

type ClientSessionView = ClientSession & {
  current: boolean
  stateLabel: string
  seenLabel: string
  expiresLabel: string
  revoking: boolean
}

function dateLabel(unixSeconds: number): string {
  if (!Number.isFinite(unixSeconds)) return '未知'
  return new Date(unixSeconds * 1_000).toLocaleDateString('zh-CN')
}

Page({
  data: {
    displayName: '',
    avatarInitial: 'C',
    username: '',
    sessions: [] as ClientSessionView[],
    loading: true,
    error: '',
    loggingOut: false,
  },

  revokingIds: [] as string[],

  async onShow() {
    finishTabSwitch(this, 3)
    await getApp<IAppOption>().authReady
    if (!sessionStore.hasToken()) {
      wx.reLaunch({ url: '/pages/bind/index' })
      return
    }
    const user = sessionStore.user()
    this.setData({
      displayName: user?.display_name || user?.username || 'ChatOS 用户',
      avatarInitial: (user?.display_name || user?.username || 'C').slice(0, 1).toUpperCase(),
      username: user?.username || '',
    })
    void this.loadSessions()
  },

  onPullDownRefresh() {
    void this.loadSessions().finally(() => wx.stopPullDownRefresh())
  },

  async loadSessions() {
    this.setData({ loading: this.data.sessions.length === 0, error: '' })
    try {
      const currentId = sessionStore.clientSessionId()
      const sessions = (await clientSessionService.list()).map((session) => ({
        ...session,
        current: session.id === currentId,
        stateLabel: session.revoked_at ? '已撤销' : session.id === currentId ? '当前设备' : '已登录',
        seenLabel: relativeTime(session.last_seen_at),
        expiresLabel: dateLabel(session.expires_at_unix),
        revoking: this.revokingIds.includes(session.id),
      }))
      this.setData({ sessions, loading: false })
    } catch (error) {
      this.setData({ loading: false, error: error instanceof Error ? error.message : '加载登录设备失败' })
    }
  },

  revokeSession(event: WechatMiniprogram.TouchEvent) {
    const id = String(event.currentTarget.dataset.id ?? '')
    const session = this.data.sessions.find((item) => item.id === id)
    if (!session || session.revoked_at || this.revokingIds.includes(id)) return
    wx.showModal({
      title: session.current ? '退出当前小程序？' : '撤销这个登录？',
      content: session.current
        ? '当前小程序需要重新通过微信登录。'
        : '该小程序上的访问会立即失效。',
      confirmText: session.current ? '退出' : '撤销登录',
      confirmColor: '#FF3B30',
      success: (result) => {
        if (result.confirm) void this.performRevoke(session)
      },
    })
  },

  async performRevoke(session: ClientSessionView) {
    this.revokingIds.push(session.id)
    await this.loadSessions()
    try {
      await clientSessionService.revoke(session.id)
      if (session.current) {
        sessionStore.clear()
        wx.reLaunch({ url: '/pages/bind/index' })
        return
      }
      await this.loadSessions()
    } catch (error) {
      this.setData({ error: error instanceof Error ? error.message : '撤销登录失败' })
    } finally {
      this.revokingIds = this.revokingIds.filter((id) => id !== session.id)
    }
  },

  logout() {
    if (this.data.loggingOut) return
    wx.showModal({
      title: '退出当前小程序？',
      content: '不会影响电脑端登录，也不会解除微信账号绑定。',
      confirmText: '退出',
      confirmColor: '#FF3B30',
      success: (result) => {
        if (result.confirm) void this.performLogout()
      },
    })
  },

  async performLogout() {
    this.setData({ loggingOut: true })
    try {
      await authService.logout()
    } finally {
      this.setData({ loggingOut: false })
      wx.reLaunch({ url: '/pages/bind/index' })
    }
  },
})
