import { beginTabSwitch, cancelTabSwitch } from '../services/tab-navigation'

Component({
  data: {
    selected: 0,
    switching: false,
    list: [
      { pagePath: '/pages/devices/index', text: '设备' },
      { pagePath: '/pages/conversations/index', text: '会话' },
      { pagePath: '/pages/agent-teams/index', text: 'Agent 团队' },
      { pagePath: '/pages/settings/index', text: '设置' },
    ],
  },

  methods: {
    switchTab(event: WechatMiniprogram.TouchEvent) {
      const index = Number(event.currentTarget.dataset.index)
      const pagePath = String(event.currentTarget.dataset.path ?? '')
      if (!Number.isInteger(index) || !pagePath || index === this.data.selected || this.data.switching) return
      const previous = this.data.selected
      this.setData({ selected: index, switching: true })
      beginTabSwitch()
      wx.switchTab({
        url: pagePath,
        fail: () => {
          cancelTabSwitch()
          this.setData({ selected: previous, switching: false })
          wx.showToast({ title: '页面切换失败，请重试', icon: 'none' })
        },
      })
    },
  },
})
