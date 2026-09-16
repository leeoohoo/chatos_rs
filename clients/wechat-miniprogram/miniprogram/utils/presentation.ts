export function relativeTime(iso?: string): string {
  if (!iso) return '暂无记录'
  const timestamp = Date.parse(iso)
  if (!Number.isFinite(timestamp)) return iso
  const seconds = Math.max(0, Math.floor((Date.now() - timestamp) / 1000))
  if (seconds < 60) return '刚刚'
  if (seconds < 3600) return `${Math.floor(seconds / 60)} 分钟前`
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)} 小时前`
  if (seconds < 604_800) return `${Math.floor(seconds / 86_400)} 天前`
  return new Date(timestamp).toLocaleDateString('zh-CN')
}

export function messageText(content: unknown): string {
  if (typeof content === 'string') return content
  if (Array.isArray(content)) {
    return content
      .map((item) => {
        if (typeof item === 'string') return item
        if (item && typeof item === 'object' && 'text' in item) return String(item.text ?? '')
        return ''
      })
      .filter(Boolean)
      .join('\n')
  }
  return content == null ? '' : JSON.stringify(content)
}
