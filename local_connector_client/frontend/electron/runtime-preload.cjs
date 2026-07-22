// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('chatosLocalRuntime', {
  apiRequest: (request) => ipcRenderer.invoke('local-connector:runtime-api-request', request),
  authenticateDesktopTicket: (ticket) => (
    ipcRenderer.invoke('local-connector:desktop-ticket-authenticate', ticket)
  ),
});
