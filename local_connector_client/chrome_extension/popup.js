// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const siteElement = document.getElementById('site');
const hostStatusElement = document.getElementById('hostStatus');
const siteStatusElement = document.getElementById('siteStatus');
const grantButton = document.getElementById('grant');
const releaseButton = document.getElementById('release');
const revokeButton = document.getElementById('revoke');

let status = null;

async function send(type, extra = {}) {
  const response = await chrome.runtime.sendMessage({ type, ...extra });
  if (!response?.ok) {
    throw new Error(response?.error || 'ChatOS Chrome 请求失败。');
  }
  return response.result;
}

async function refresh() {
  try {
    status = await send('popup_status');
    render();
  } catch (error) {
    siteStatusElement.textContent = error instanceof Error ? error.message : String(error);
    siteStatusElement.className = 'status warn';
  }
}

function render() {
  siteElement.textContent = status?.origin || '当前页面不支持连接';
  hostStatusElement.textContent = status?.connected_to_native_host
    ? 'Native Host 已连接'
    : 'Native Host 未连接，请先在 Local Connector 中启用 Chrome 整合';
  hostStatusElement.className = `status ${status?.connected_to_native_host ? 'ready' : 'warn'}`;
  siteStatusElement.textContent = status?.claimed
    ? '当前标签页已连接'
    : status?.permission_granted
      ? '站点已授权，标签页尚未连接'
      : '当前站点尚未授权';
  siteStatusElement.className = `status ${status?.claimed ? 'ready' : ''}`;
  grantButton.disabled = !status?.tab_supported || status?.claimed;
  releaseButton.disabled = !status?.claimed;
  revokeButton.disabled = !status?.permission_granted || !status?.pattern;
}

grantButton.addEventListener('click', async () => {
  if (!status?.pattern) return;
  grantButton.disabled = true;
  try {
    const granted = await chrome.permissions.request({ origins: [status.pattern] });
    if (!granted) {
      throw new Error('未授予当前站点权限。');
    }
    status = await send('claim_active_tab');
    render();
  } catch (error) {
    siteStatusElement.textContent = error instanceof Error ? error.message : String(error);
    siteStatusElement.className = 'status warn';
  } finally {
    grantButton.disabled = false;
  }
});

releaseButton.addEventListener('click', async () => {
  releaseButton.disabled = true;
  try {
    status = await send('release_active_tab');
    render();
  } catch (error) {
    siteStatusElement.textContent = error instanceof Error ? error.message : String(error);
    siteStatusElement.className = 'status warn';
  }
});

revokeButton.addEventListener('click', async () => {
  if (!status?.pattern || !status?.origin) return;
  revokeButton.disabled = true;
  try {
    await chrome.permissions.remove({ origins: [status.pattern] });
    status = await send('origin_permission_removed', { origin: status.origin });
    render();
  } catch (error) {
    siteStatusElement.textContent = error instanceof Error ? error.message : String(error);
    siteStatusElement.className = 'status warn';
  }
});

void refresh();
