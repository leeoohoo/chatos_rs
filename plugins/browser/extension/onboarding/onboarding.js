const statusDot = document.querySelector('#status-dot');
const statusTitle = document.querySelector('#status-title');
const statusDetail = document.querySelector('#status-detail');
const connectButton = document.querySelector('#connect');
const closeButton = document.querySelector('#close');
const error = document.querySelector('#error');

connectButton.addEventListener('click', connect);
closeButton.addEventListener('click', closeCurrentTab);

await refresh();
setInterval(refresh, 1500);

async function connect() {
  connectButton.disabled = true;
  error.hidden = true;
  try {
    const response = await chrome.runtime.sendMessage({action: 'connect'});
    if (!response?.ok) throw response?.error;
    render(response.result);
  } catch (reason) {
    showError(friendlyError(reason));
  } finally {
    connectButton.disabled = false;
  }
}

async function refresh() {
  try {
    const response = await chrome.runtime.sendMessage({action: 'status'});
    if (response?.ok) render(response.result);
  } catch {
    showError('扩展后台暂时不可用，请刷新本页后重试。');
  }
}

function render(state) {
  const connected = Boolean(state?.connected);
  statusDot.classList.toggle('connected', connected);
  closeButton.hidden = !connected;
  connectButton.hidden = connected;
  if (connected) {
    statusTitle.textContent = '连接成功';
    statusDetail.textContent = '现在可以返回 Chatos，浏览器任务会自动创建任务标签组。';
    error.hidden = true;
  } else if (state?.paired) {
    statusTitle.textContent = '已授权，正在等待 Browser MCP';
    statusDetail.textContent = '在 Chatos 中启动浏览器任务后，本页会自动显示已连接。';
  } else {
    statusTitle.textContent = '等待一次性本机授权';
    statusDetail.textContent = '先在 Chatos 中启动浏览器任务，再点击“连接并继续”。';
  }
}

function friendlyError(reason) {
  if (reason?.code === 'extension_unavailable') {
    return '尚未检测到正在运行的 Browser MCP。请先在 Chatos 中发起一个浏览器任务，然后再次点击“连接并继续”。';
  }
  if (reason?.code === 'permission_denied') {
    return '连接未获授权，请确认安装的是正式版 Chatos Browser Bridge，并在 Chatos 中允许“连接现有 Chrome”。';
  }
  return reason?.message ?? '连接失败，请确认 Chatos 和 Browser MCP 正在运行。';
}

function showError(message) {
  error.textContent = message;
  error.hidden = false;
}

async function closeCurrentTab() {
  const tab = await chrome.tabs.getCurrent();
  if (Number.isSafeInteger(tab?.id)) await chrome.tabs.remove(tab.id);
}
