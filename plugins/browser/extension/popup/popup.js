const status = document.querySelector('#status');
const statusDot = document.querySelector('#status-dot');
const connectButton = document.querySelector('#connect');
const shareButton = document.querySelector('#share');
const disconnectButton = document.querySelector('#disconnect');
const targets = document.querySelector('#targets');
const error = document.querySelector('#error');

connectButton.addEventListener('click', () => runAction('connect'));
shareButton.addEventListener('click', () => runAction('share_current_tab'));
disconnectButton.addEventListener('click', () => runAction('disconnect'));

void refresh();

async function refresh() {
  const response = await chrome.runtime.sendMessage({action: 'status'});
  applyResponse(response);
}

async function runAction(action, extra = {}) {
  setBusy(true);
  error.hidden = true;
  try {
    const response = await chrome.runtime.sendMessage({action, ...extra});
    applyResponse(response);
  } catch {
    showError('The extension service worker is unavailable.');
  } finally {
    setBusy(false);
  }
}

function applyResponse(response) {
  if (!response?.ok) {
    showError(friendlyError(response?.error));
    return;
  }
  const state = response.result;
  status.textContent = connectionLabel(state);
  statusDot.classList.toggle('connected', state.connected);
  connectButton.disabled = state.connected;
  shareButton.disabled = !state.connected;
  disconnectButton.disabled = !state.connected;
  renderTargets(state.targets ?? []);
}

function renderTargets(items) {
  targets.replaceChildren();
  for (const target of items) {
    const item = document.createElement('li');
    const label = document.createElement('span');
    label.className = 'tab-label';
    const title = document.createElement('span');
    title.className = 'tab-title';
    title.textContent = target.title || 'Untitled tab';
    const url = document.createElement('span');
    url.className = 'tab-url';
    url.textContent = target.url || '';
    const revoke = document.createElement('button');
    revoke.className = 'revoke';
    revoke.type = 'button';
    revoke.textContent = 'Revoke';
    revoke.addEventListener('click', () => runAction('revoke_target', {target_id: target.id}));
    label.append(title, url);
    item.append(label, revoke);
    targets.append(item);
  }
}

function setBusy(busy) {
  if (busy) {
    connectButton.disabled = true;
    shareButton.disabled = true;
    disconnectButton.disabled = true;
  }
}

function showError(message) {
  error.textContent = message;
  error.hidden = false;
}

function connectionLabel(state) {
  if (state.connected) return '已连接，可以在 ChatOS 中使用';
  if (state.paired) return '已配对，正在等待 Browser MCP 自动重连';
  return '尚未配对，请按下方步骤完成首次连接';
}

function friendlyError(protocolError) {
  if (protocolError?.code === 'extension_unavailable') {
    return '未找到正在运行的 Browser MCP。请先在 ChatOS 中启动一个浏览器任务，然后重试。';
  }
  if (protocolError?.code === 'permission_denied') {
    return '连接未获授权，请检查 ChatOS 中 Browser CDP 的“连接现有 Chrome”权限。';
  }
  return protocolError?.message ?? '操作失败，请确认 ChatOS 和 Local Connector 正在运行。';
}
