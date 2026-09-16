import { apiOrigin } from '../config/runtime'
import { sessionStore } from '../stores/session-store'

type ServiceSurface = 'user' | 'chatos' | 'local'
type HttpMethod = 'GET' | 'POST' | 'DELETE'

export class ApiError extends Error {
  constructor(
    message: string,
    readonly statusCode: number,
    readonly code?: string,
  ) {
    super(message)
  }
}

function servicePath(surface: ServiceSurface, path: string): string {
  const prefix = surface === 'user' ? '/api/user' : surface === 'chatos' ? '/api/chatos' : '/api/local'
  return `${prefix}${path.startsWith('/') ? path : `/${path}`}`
}

export async function apiRequest<T>(options: {
  surface: ServiceSurface
  path: string
  method?: HttpMethod
  data?: WechatMiniprogram.IAnyObject | string | ArrayBuffer
  authenticated?: boolean
  headers?: Record<string, string>
}): Promise<T> {
  const authenticated = options.authenticated !== false
  const token = authenticated ? sessionStore.token() : undefined
  if (authenticated && !token) {
    throw new ApiError('登录状态已失效，请重新登录', 401)
  }
  return new Promise<T>((resolve, reject) => {
    wx.request({
      url: `${apiOrigin()}${servicePath(options.surface, options.path)}`,
      method: options.method ?? 'GET',
      data: options.data,
      timeout: 15_000,
      header: {
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
        ...(options.headers ?? {}),
      },
      success(response) {
        const status = response.statusCode
        const body = response.data as Record<string, unknown> | undefined
        if (status >= 200 && status < 300) {
          resolve(response.data as T)
          return
        }
        if (status === 401) {
          sessionStore.clear()
          wx.reLaunch({ url: '/pages/bind/index' })
        }
        const message =
          (typeof body?.error === 'string' && body.error) ||
          (typeof body?.message === 'string' && body.message) ||
          `请求失败（${status}）`
        reject(new ApiError(message, status, typeof body?.code === 'string' ? body.code : undefined))
      },
      fail(error) {
        reject(new ApiError(error.errMsg || '网络连接失败', 0))
      },
    })
  })
}
