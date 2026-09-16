interface IAppOption {
  globalData: {
    launchScene?: string
  }
  authReady: Promise<void>
  restoreSession(): Promise<void>
}
