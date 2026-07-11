// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('chatosLocalConnector', {
  apiRequest: (request) => ipcRenderer.invoke('local-connector:api-request', request),
  openSettings: () => ipcRenderer.invoke('local-connector:settings-open'),
  reloadChatOS: () => ipcRenderer.invoke('local-connector:chatos-reload'),
});
