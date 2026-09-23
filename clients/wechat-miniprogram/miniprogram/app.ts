import { authService } from './services/auth-service'
import { realtimeClient } from './services/realtime-client'
import { sessionStore } from './stores/session-store'

App<IAppOption>({
  globalData: {
    launchScene: undefined,
  },
  authReady: Promise.resolve(),

  onLaunch(options) {
    this.globalData.launchScene = options.query?.scene
    this.authReady = this.restoreSession()
  },

  onHide() {
    realtimeClient.close()
  },

  async restoreSession() {
    if (sessionStore.hasToken()) {
      return
    }
    try {
      const result = await authService.login()
      if (result.status === 'binding_required') return
    } catch {
      // Pages expose a retry action. Startup must remain usable during a transient outage.
    }
  },
})
