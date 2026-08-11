// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

async function loadChatosPage({ webContents, url, bypassCache }) {
  if (bypassCache) {
    await webContents.session.clearCache();
  }
  return webContents.loadURL(url);
}

function reloadChatosPage({ webContents, bypassCache }) {
  if (bypassCache && typeof webContents.reloadIgnoringCache === 'function') {
    webContents.reloadIgnoringCache();
    return;
  }
  webContents.reload();
}

module.exports = {
  loadChatosPage,
  reloadChatosPage,
};
