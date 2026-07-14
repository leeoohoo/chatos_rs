// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('chatosLocalConnector', {
  apiRequest: (request) => ipcRenderer.invoke('local-connector:api-request', request),
  getDesktopSystemPermissions: () => ipcRenderer.invoke('local-connector:desktop-system-permissions'),
  requestDesktopSystemPermission: (permissionId) => (
    ipcRenderer.invoke('local-connector:desktop-system-permission-request', permissionId)
  ),
  openSettings: () => ipcRenderer.invoke('local-connector:settings-open'),
  closeSettings: () => ipcRenderer.invoke('local-connector:settings-close'),
  reloadChatOS: () => ipcRenderer.invoke('local-connector:chatos-reload'),
  setDeveloperMode: (enabled) => ipcRenderer.invoke('local-connector:developer-mode', enabled),
});
