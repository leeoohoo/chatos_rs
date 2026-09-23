import { apiOrigin } from '../config/runtime'
import { sessionStore } from '../stores/session-store'
import { deviceIdentityStore } from '../security/device-identity'

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
  clearSessionOnUnauthorized?: boolean
  headers?: Record<string, string>
}): Promise<T> {
  const authenticated = options.authenticated !== false
  const token = authenticated ? sessionStore.token() : undefined
  if (authenticated && !token) {
    throw new ApiError('登录状态已失效，请重新登录', 401)
  }
  const method = options.method ?? 'GET'
  const target = servicePath(options.surface, options.path)
  const data = serializeRequestData(options.data)
  const proofHeaders = authenticated
    ? await createDeviceProofHeaders(options.surface, method, target, data)
    : {}
  return new Promise<T>((resolve, reject) => {
    wx.request({
      url: `${apiOrigin()}${target}`,
      method,
      data,
      timeout: 15_000,
      header: {
        ...(typeof options.data === 'object' && !(options.data instanceof ArrayBuffer)
          ? { 'Content-Type': 'application/json' }
          : {}),
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
        ...proofHeaders,
        ...(options.headers ?? {}),
      },
      success(response) {
        const status = response.statusCode
        const body = response.data as Record<string, unknown> | undefined
        if (status >= 200 && status < 300) {
          resolve(response.data as T)
          return
        }
        if (status === 401 && options.clearSessionOnUnauthorized !== false) {
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

function serializeRequestData(
  data: WechatMiniprogram.IAnyObject | string | ArrayBuffer | undefined,
): string | ArrayBuffer | undefined {
  if (data === undefined || typeof data === 'string' || data instanceof ArrayBuffer) return data
  return JSON.stringify(data)
}

async function createDeviceProofHeaders(
  surface: ServiceSurface,
  method: HttpMethod,
  target: string,
  data: string | ArrayBuffer | undefined,
): Promise<Record<string, string>> {
  const clientSessionId = sessionStore.clientSessionId()
  if (!clientSessionId) throw new ApiError('设备绑定会话已失效，请重新绑定', 401)
  const identity = await deviceIdentityStore.identity()
  const timestamp = Math.floor(Date.now() / 1000)
  const nonce = await deviceIdentityStore.nonce()
  const bodyDigest = deviceIdentityStore.bodyDigest(data)
  const payload = [
    'chatos-device-proof-v1',
    surface,
    method,
    target,
    bodyDigest,
    clientSessionId,
    identity.deviceId,
    String(timestamp),
    nonce,
  ].join('\n')
  const signature = await deviceIdentityStore.sign(payload)
  return {
    'X-Chatos-Device-Id': identity.deviceId,
    'X-Chatos-Device-Session-Id': clientSessionId,
    'X-Chatos-Device-Timestamp': String(timestamp),
    'X-Chatos-Device-Nonce': nonce,
    'X-Chatos-Device-Body-SHA512': bodyDigest,
    'X-Chatos-Device-Signature-Alg': 'ed25519',
    'X-Chatos-Device-Signature': signature,
  }
}
