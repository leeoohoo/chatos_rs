type ExtConfig = {
  apiOrigin?: string
}

const DEFAULT_API_ORIGIN = 'https://app.jgoool.com'
const DEVELOPMENT_API_ORIGIN = DEFAULT_API_ORIGIN

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
    (development ? DEVELOPMENT_API_ORIGIN : DEFAULT_API_ORIGIN)
  )
}

export function websocketOrigin(): string {
  return apiOrigin().replace(/^https:/, 'wss:').replace(/^http:/, 'ws:')
}
