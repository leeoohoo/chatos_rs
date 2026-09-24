let tabSwitchLoading = false
let loadingTimeout: ReturnType<typeof setTimeout> | undefined

type TabBarHandle = {
  setData(data: { selected: number; switching: boolean }): void
}

type TabPage = {
  getTabBar?: () => TabBarHandle | undefined
}

export function beginTabSwitch(): void {
  if (loadingTimeout) clearTimeout(loadingTimeout)
  tabSwitchLoading = true
  wx.showLoading({ title: '加载中', mask: true })
  loadingTimeout = setTimeout(() => {
    loadingTimeout = undefined
    if (!tabSwitchLoading) return
    tabSwitchLoading = false
    wx.hideLoading()
  }, 10_000)
}

export function finishTabSwitch(page: TabPage, selected: number): void {
  page.getTabBar?.()?.setData({ selected, switching: false })
  if (!tabSwitchLoading) return
  tabSwitchLoading = false
  if (loadingTimeout) clearTimeout(loadingTimeout)
  loadingTimeout = undefined
  // Keep the native overlay through the target page's first paint. The page's
  // synchronous cache setData joins this tick, so users never see a white gap.
  wx.nextTick(() => {
    if (!tabSwitchLoading) wx.hideLoading()
  })
}

export function cancelTabSwitch(): void {
  if (!tabSwitchLoading) return
  tabSwitchLoading = false
  if (loadingTimeout) clearTimeout(loadingTimeout)
  loadingTimeout = undefined
  wx.hideLoading()
}
