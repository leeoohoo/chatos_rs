import type { AuthUser } from '../models/api'

const TOKEN_KEY = 'chatos.companion.token.v1'
const USER_KEY = 'chatos.companion.user.v1'
const CLIENT_SESSION_KEY = 'chatos.companion.client-session.v1'

class SessionStore {
  token(): string | undefined {
    const value = wx.getStorageSync<string>(TOKEN_KEY)
    return typeof value === 'string' && value.trim() ? value : undefined
  }

  user(): AuthUser | undefined {
    const value = wx.getStorageSync<AuthUser>(USER_KEY)
    return value && typeof value.id === 'string' ? value : undefined
  }

  clientSessionId(): string | undefined {
    const value = wx.getStorageSync<string>(CLIENT_SESSION_KEY)
    return typeof value === 'string' && value.trim() ? value : undefined
  }

  hasToken(): boolean {
    return Boolean(this.token())
  }

  save(token: string, user: AuthUser, clientSessionId: string): void {
    wx.setStorageSync(TOKEN_KEY, token)
    wx.setStorageSync(USER_KEY, user)
    wx.setStorageSync(CLIENT_SESSION_KEY, clientSessionId)
  }

  clear(): void {
    wx.removeStorageSync(TOKEN_KEY)
    wx.removeStorageSync(USER_KEY)
    wx.removeStorageSync(CLIENT_SESSION_KEY)
  }
}

export const sessionStore = new SessionStore()
