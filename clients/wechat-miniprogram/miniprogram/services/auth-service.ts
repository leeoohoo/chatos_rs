import type { BindClaimResponse, BindClaimResult, WeChatLoginResponse } from '../models/api'
import { sessionStore } from '../stores/session-store'
import { apiRequest } from './api-client'

function wxLoginCode(): Promise<string> {
  return new Promise((resolve, reject) => {
    wx.login({
      timeout: 10_000,
      success(result) {
        result.code ? resolve(result.code) : reject(new Error('微信登录未返回有效 code'))
      },
      fail: reject,
    })
  })
}

function persistAuthenticated(result: WeChatLoginResponse | BindClaimResult): void {
  if (result.status === 'authenticated') {
    sessionStore.save(result.token, result.user, result.client_session_id)
  }
}

class AuthService {
  async developmentLogin(username: string, password: string): Promise<WeChatLoginResponse> {
    const result = await apiRequest<WeChatLoginResponse>({
      surface: 'user',
      path: '/auth/wechat/mini-program/development-login',
      method: 'POST',
      data: { username, password },
      authenticated: false,
    })
    persistAuthenticated(result)
    return result
  }

  async login(): Promise<WeChatLoginResponse> {
    const code = await wxLoginCode()
    const result = await apiRequest<WeChatLoginResponse>({
      surface: 'user',
      path: '/auth/wechat/mini-program/login',
      method: 'POST',
      data: { code },
      authenticated: false,
    })
    persistAuthenticated(result)
    return result
  }

  async claim(bindTicket: string): Promise<BindClaimResponse> {
    const code = await wxLoginCode()
    return apiRequest<BindClaimResponse>({
      surface: 'user',
      path: '/auth/wechat/mini-program/bind-claims',
      method: 'POST',
      data: { code, bind_ticket: bindTicket },
      authenticated: false,
    })
  }

  async claimResult(claimId: string, claimSecret: string): Promise<BindClaimResult> {
    const result = await apiRequest<BindClaimResult>({
      surface: 'user',
      path: `/auth/wechat/mini-program/bind-claims/${encodeURIComponent(claimId)}/result`,
      method: 'POST',
      data: { claim_secret: claimSecret },
      authenticated: false,
    })
    persistAuthenticated(result)
    return result
  }

  async logout(): Promise<void> {
    try {
      await apiRequest<void>({ surface: 'user', path: '/auth/logout', method: 'POST' })
    } finally {
      sessionStore.clear()
    }
  }
}

export const authService = new AuthService()
