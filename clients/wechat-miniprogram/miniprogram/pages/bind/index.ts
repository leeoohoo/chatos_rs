import { authService } from '../../services/auth-service'
import { ApiError } from '../../services/api-client'
import { isDevelopmentEnvironment } from '../../config/runtime'
import { sessionStore } from '../../stores/session-store'

type BindState = 'ready' | 'claiming' | 'development_login' | 'waiting' | 'failed'

Page({
  data: {
    scene: '',
    state: 'ready' as BindState,
    error: '',
    developmentMode: false,
    developmentUsername: 'admin',
    developmentPassword: '',
  },

  pollTimer: undefined as ReturnType<typeof setTimeout> | undefined,
  pollInFlight: false,
  pollFailures: 0,
  expiresAt: 0,
  claimId: '',
  claimSecret: '',

  onLoad(query: Record<string, string | undefined>) {
    if (sessionStore.hasToken()) {
      void wx.switchTab({ url: '/pages/devices/index' })
      return
    }
    const app = getApp<IAppOption>()
    const scene = decodeURIComponent(query.scene ?? app.globalData.launchScene ?? '').trim()
    this.setData({ scene, developmentMode: isDevelopmentEnvironment() })
    if (scene) void this.startClaim()
  },

  onDevelopmentUsernameInput(event: WechatMiniprogram.Input) {
    this.setData({ developmentUsername: event.detail.value })
  },

  onDevelopmentPasswordInput(event: WechatMiniprogram.Input) {
    this.setData({ developmentPassword: event.detail.value })
  },

  async startDevelopmentLogin() {
    if (!this.data.developmentMode || this.data.state === 'development_login') return
    const username = this.data.developmentUsername.trim()
    if (!username || !this.data.developmentPassword) {
      this.setData({ error: '请输入本地 ChatOS 的用户名和密码' })
      return
    }
    this.setData({ state: 'development_login', error: '' })
    try {
      const result = await authService.developmentLogin(username, this.data.developmentPassword)
      if (result.status !== 'authenticated') {
        throw new Error('本地测试登录未返回有效会话')
      }
      wx.switchTab({ url: '/pages/devices/index' })
    } catch (error) {
      this.setData({
        state: 'ready',
        error: error instanceof Error ? error.message : '本地测试登录失败',
      })
    }
  },

  onUnload() {
    this.stopPolling()
  },

  onHide() {
    this.stopPolling()
  },

  onShow() {
    if (this.data.state === 'waiting' && this.claimId && this.claimSecret) {
      this.schedulePoll(0)
    }
  },

  async startClaim() {
    if (!this.data.scene || this.data.state === 'claiming') return
    this.setData({ state: 'claiming', error: '' })
    try {
      const existing = await authService.login()
      if (existing.status === 'authenticated') {
        wx.switchTab({ url: '/pages/devices/index' })
        return
      }
      const claim = await authService.claim(this.data.scene)
      this.claimId = claim.claim_id
      this.claimSecret = claim.claim_secret
      this.expiresAt = claim.expires_at_unix
      this.pollFailures = 0
      this.setData({ state: 'waiting' })
      this.schedulePoll(0)
    } catch (error) {
      this.setData({ state: 'failed', error: error instanceof Error ? error.message : '绑定失败' })
    }
  },

  async pollResult() {
    if (!this.claimId || !this.claimSecret || this.pollInFlight) return
    if (Date.now() / 1000 >= this.expiresAt) {
      this.stopPolling()
      this.setData({ state: 'failed', error: '绑定请求已过期，请回到桌面重新生成二维码' })
      return
    }
    this.pollInFlight = true
    try {
      const result = await authService.claimResult(this.claimId, this.claimSecret)
      if (result.status === 'authenticated') {
        this.stopPolling()
        wx.switchTab({ url: '/pages/devices/index' })
        return
      }
      this.pollFailures = 0
      this.setData({ error: '' })
      this.schedulePoll(2_000)
    } catch (error) {
      if (error instanceof ApiError && [400, 403, 404, 409].includes(error.statusCode)) {
        await this.recoverCompletedBinding(error)
        return
      }
      this.pollFailures += 1
      this.setData({ error: '网络暂时不可用，仍在等待桌面确认…' })
      const retryDelay = Math.min(8_000, 1_000 * (2 ** Math.min(this.pollFailures, 3)))
      this.schedulePoll(retryDelay)
    } finally {
      this.pollInFlight = false
    }
  },

  async recoverCompletedBinding(originalError: Error) {
    try {
      const login = await authService.login()
      if (login.status === 'authenticated') {
        this.stopPolling()
        wx.switchTab({ url: '/pages/devices/index' })
        return
      }
    } catch {
      // Preserve the original claim error; it gives the user the actionable state.
    }
    this.stopPolling()
    this.setData({ state: 'failed', error: originalError.message || '确认绑定失败' })
  },

  schedulePoll(delay: number) {
    if (this.pollTimer) clearTimeout(this.pollTimer)
    this.pollTimer = setTimeout(() => void this.pollResult(), delay)
  },

  stopPolling() {
    if (this.pollTimer) clearTimeout(this.pollTimer)
    this.pollTimer = undefined
  },
})
