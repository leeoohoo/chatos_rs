type ExtConfig = {
  apiOrigin?: string
}

const DEFAULT_API_ORIGIN = 'https://app.jgoool.com'
const LOCAL_DEVELOPMENT_API_ORIGIN = 'http://127.0.0.1:9080'

export function isDevelopmentEnvironment(): boolean {
  try {
    return wx.getAccountInfoSync().miniProgram.envVersion === 'develop'
  } catch {
    return false
  }
}

function normalizeOrigin(value: unknown, allowLocalHttp: boolean): string | undefined {
  if (typeof value !== 'string') return undefined
  const origin = value.trim().replace(/\/+$/, '')
  if (/^https:\/\//.test(origin)) return origin
  if (allowLocalHttp && /^http:\/\/(127\.0\.0\.1|localhost)(:\d+)?$/.test(origin)) return origin
  return undefined
}

export function apiOrigin(): string {
  const development = isDevelopmentEnvironment()
  const ext = wx.getExtConfigSync?.() as ExtConfig | undefined
  return (
    normalizeOrigin(ext?.apiOrigin, development) ??
    (development ? LOCAL_DEVELOPMENT_API_ORIGIN : DEFAULT_API_ORIGIN)
  )
}

export function websocketOrigin(): string {
  return apiOrigin().replace(/^https:/, 'wss:').replace(/^http:/, 'ws:')
}
