// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('chatosLocalConnector', {
  apiRequest: (request) => ipcRenderer.invoke('local-connector:api-request', request),
  runtimeApiRequest: (request) => ipcRenderer.invoke('local-connector:runtime-api-request', request),
  selectPluginFiles: () => ipcRenderer.invoke('local-connector:select-plugin-files'),
  openSettings: (tab) => ipcRenderer.invoke('local-connector:settings-open', tab),
  closeSettings: () => ipcRenderer.invoke('local-connector:settings-close'),
  setApprovalOverlayMode: (mode) => (
    ipcRenderer.invoke('local-connector:approval-overlay-mode', mode)
  ),
  setVisualPreviewMode: (mode) => (
    ipcRenderer.invoke('local-connector:visual-preview-mode', mode)
  ),
  onSettingsTabRequested: (callback) => {
    const listener = (_event, tab) => callback(tab);
    ipcRenderer.on('local-connector:settings-tab', listener);
    return () => ipcRenderer.removeListener('local-connector:settings-tab', listener);
  },
  reloadChatOS: () => ipcRenderer.invoke('local-connector:chatos-reload'),
  setDeveloperMode: (enabled) => ipcRenderer.invoke('local-connector:developer-mode', enabled),
  runtimeSettings: () => ipcRenderer.invoke('local-connector:runtime-settings'),
  updateRuntimeSettings: (payload) => (
    ipcRenderer.invoke('local-connector:runtime-settings-update', payload)
  ),
});
