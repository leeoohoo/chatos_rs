// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const {
  app,
  BrowserWindow,
  Menu,
  WebContentsView,
  ipcMain,
  session,
  shell,
  systemPreferences,
} = require('electron');
const { spawn } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { createLocalApiBridge } = require('./local-api-bridge.cjs');

let mainWindow = null;
let settingsView = null;
let settingsOpen = false;
let chatosView = null;
let coreProcess = null;
const desktopAuthToken = crypto.randomBytes(32).toString('base64url');
let ipcEndpoint = null;
let ipcSocketDir = null;
let developerMode = process.env.LOCAL_CONNECTOR_DEVELOPER_MODE === '1';
const trustedLocalWebContents = new Set();
const localApiBridge = createLocalApiBridge({
  getIpcEndpoint: () => ipcEndpoint,
  getDesktopAuthToken: () => desktopAuthToken,
  isTrustedSender: (sender) => isTrustedLocalSender(sender),
});
const {
  delay,
  requestLocalApiOverIpc,
  sendIpcHttpRequest,
  localApiHeaders,
} = localApiBridge;
const SHELL_HEIGHT = 52;
const MAX_UNIX_SOCKET_PATH_BYTES = 100;
const CORE_LOG_MAX_BYTES = 5 * 1024 * 1024;
const DEVELOPER_CHATOS_WEB_URL = (
  process.env.LOCAL_CONNECTOR_DEVELOPER_CHATOS_WEB_URL || 'http://127.0.0.1:8088'
).trim();
const DEVELOPER_CLOUD_BASE_URL = (
  process.env.LOCAL_CONNECTOR_DEVELOPER_CLOUD_BASE_URL || 'http://127.0.0.1:39230'
).trim();

function resourcePath(...segments) {
  const packagedPath = path.join(process.resourcesPath, ...segments);
  if (fs.existsSync(packagedPath)) {
    return packagedPath;
  }
  return path.join(__dirname, 'resources', ...segments);
}

function startCore() {
  if (!ipcEndpoint) {
    ipcEndpoint = createIpcEndpoint();
    ipcSocketDir = process.platform === 'win32' ? null : path.dirname(ipcEndpoint);
  }
  const coreName = process.platform === 'win32'
    ? 'local_connector_client_core.exe'
    : 'local_connector_client_core';
  const corePath = resourcePath(coreName);
  const env = {
    ...process.env,
    CHATOS_BUNDLED_TOOLS_DIR: resourcePath('bundled-tools'),
    CHATOS_BUNDLED_SKILLS_DIR: resourcePath('skill-bundles'),
    LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN: desktopAuthToken,
    LOCAL_CONNECTOR_IPC_ENDPOINT: ipcEndpoint,
    LOCAL_CONNECTOR_OPEN_UI: '0',
    LOCAL_CONNECTOR_REQUIRE_SECURE_REMOTE: process.env.LOCAL_CONNECTOR_REQUIRE_SECURE_REMOTE || '1',
  };

  const coreLog = openCoreLog();
  try {
    coreProcess = spawn(corePath, [], {
      cwd: path.dirname(corePath),
      env,
      stdio: coreLog.fd === null ? 'ignore' : ['ignore', coreLog.fd, coreLog.fd],
      windowsHide: true,
    });
  } finally {
    if (coreLog.fd !== null) {
      fs.closeSync(coreLog.fd);
    }
  }

  coreProcess.on('error', (error) => {
    appendCoreLog(coreLog.path, `core process failed to start: ${error.stack || error}`);
  });
  coreProcess.on('exit', (code, signal) => {
    appendCoreLog(coreLog.path, `core process exited: code=${code} signal=${signal}`);
    coreProcess = null;
  });
}

function createIpcEndpoint() {
  const suffix = `${process.pid}-${crypto.randomBytes(24).toString('hex')}`;
  if (process.platform === 'win32') {
    return `\\\\.\\pipe\\chatos-local-connector-${suffix}`;
  }

  const candidateRoots = [...new Set([app.getPath('temp'), '/tmp'])];
  let lastError = null;
  for (const root of candidateRoots) {
    let socketDir = null;
    try {
      fs.mkdirSync(root, { recursive: true });
      socketDir = fs.mkdtempSync(path.join(root, 'chatos-'));
      fs.chmodSync(socketDir, 0o700);
      const endpoint = path.join(socketDir, 'core.sock');
      if (Buffer.byteLength(endpoint) <= MAX_UNIX_SOCKET_PATH_BYTES) {
        return endpoint;
      }
      lastError = new Error(`Local connector IPC socket path is too long: ${endpoint}`);
    } catch (error) {
      lastError = error;
    }
    if (socketDir) {
      fs.rmSync(socketDir, { recursive: true, force: true });
    }
  }
  throw lastError || new Error('Unable to create a local connector IPC socket path');
}

function openCoreLog() {
  try {
    const logsDir = app.getPath('logs');
    fs.mkdirSync(logsDir, { recursive: true });
    const logPath = path.join(logsDir, 'local-connector-core.log');
    if (fs.existsSync(logPath) && fs.statSync(logPath).size > CORE_LOG_MAX_BYTES) {
      const previousLogPath = `${logPath}.1`;
      fs.rmSync(previousLogPath, { force: true });
      fs.renameSync(logPath, previousLogPath);
    }
    const fd = fs.openSync(logPath, 'a', 0o600);
    fs.chmodSync(logPath, 0o600);
    fs.writeSync(fd, `\n[${new Date().toISOString()}] starting local connector core\n`);
    return { fd, path: logPath };
  } catch (error) {
    console.error('Unable to open Local Connector Core log', error);
    return { fd: null, path: null };
  }
}

function appendCoreLog(logPath, message) {
  if (!logPath) {
    console.error(message);
    return;
  }
  try {
    fs.appendFileSync(logPath, `[${new Date().toISOString()}] ${message}\n`, { mode: 0o600 });
  } catch (error) {
    console.error('Unable to append Local Connector Core log', error);
  }
}

function isTrustedLocalSender(sender) {
  return Boolean(sender && trustedLocalWebContents.has(sender.id));
}

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1180,
    height: 780,
    minWidth: 920,
    minHeight: 620,
    title: 'Chat OS Local Connector',
    backgroundColor: '#121214',
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      preload: path.join(__dirname, 'preload.cjs'),
      sandbox: true,
    },
  });
  const mainWebContentsId = mainWindow.webContents.id;
  trustedLocalWebContents.add(mainWebContentsId);

  mainWindow.loadFile(path.join(__dirname, '..', 'dist', 'index.html'), {
    query: { view: 'shell' },
  });
  mainWindow.on('resize', layoutMainViews);
  mainWindow.on('focus', restoreMainWindowContent);
  mainWindow.on('show', restoreMainWindowContent);
  mainWindow.on('closed', () => {
    trustedLocalWebContents.delete(mainWebContentsId);
    if (settingsView && !settingsView.webContents.isDestroyed()) {
      trustedLocalWebContents.delete(settingsView.webContents.id);
    }
    settingsView = null;
    settingsOpen = false;
    chatosView = null;
    mainWindow = null;
  });
  createChatosView();

  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url);
    return { action: 'deny' };
  });
}

function createChatosView() {
  const partition = developerMode ? 'persist:chatos-web-development' : 'persist:chatos-web';
  session.fromPartition(partition).setPermissionRequestHandler((_webContents, _permission, callback) => {
    callback(false);
  });
  chatosView = new WebContentsView({
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      partition,
      backgroundThrottling: false,
    },
  });
  const createdView = chatosView;
  mainWindow.contentView.addChildView(chatosView);
  chatosView.setVisible(!settingsOpen);
  layoutChatosView();

  chatosView.webContents.on('will-navigate', (event, url) => {
    if (handleChatosProtocolNavigation(url)) {
      event.preventDefault();
      return;
    }
    if (!isAllowedChatosUrl(url)) {
      event.preventDefault();
      openExternalIfSafe(url);
    }
  });
  chatosView.webContents.setWindowOpenHandler(({ url }) => {
    if (handleChatosProtocolNavigation(url)) {
      return { action: 'deny' };
    }
    openExternalIfSafe(url);
    return { action: 'deny' };
  });
  chatosView.webContents.on('render-process-gone', () => {
    if (chatosView !== createdView || !mainWindow || mainWindow.isDestroyed()) {
      return;
    }
    try {
      mainWindow.contentView.removeChildView(createdView);
    } catch {
      // The failed renderer may already have been detached.
    }
    chatosView = null;
    if (!createdView.webContents.isDestroyed()) {
      createdView.webContents.close();
    }
    if (!settingsOpen) {
      setTimeout(() => {
        if (mainWindow && !mainWindow.isDestroyed() && !chatosView) {
          createChatosView();
          repaintView(chatosView);
        }
      }, 100);
    }
  });
  chatosView.webContents.loadURL(chatosUrlWithDesktopParam());
}

function recreateChatosView() {
  if (!mainWindow || mainWindow.isDestroyed()) {
    return;
  }
  if (chatosView && !chatosView.webContents.isDestroyed()) {
    try {
      mainWindow.contentView.removeChildView(chatosView);
    } catch {
      // The view may already have been detached while the window was closing.
    }
    chatosView.webContents.close();
  }
  chatosView = null;
  createChatosView();
}

function layoutChatosView() {
  if (!mainWindow || !chatosView) {
    return;
  }
  const bounds = mainWindow.getContentBounds();
  chatosView.setBounds({
    x: 0,
    y: SHELL_HEIGHT,
    width: bounds.width,
    height: Math.max(0, bounds.height - SHELL_HEIGHT),
  });
}

function layoutSettingsView() {
  if (!mainWindow || !settingsView) {
    return;
  }
  const bounds = mainWindow.getContentBounds();
  settingsView.setBounds({
    x: 0,
    y: 0,
    width: bounds.width,
    height: bounds.height,
  });
}

function layoutMainViews() {
  layoutChatosView();
  layoutSettingsView();
}

function repaintView(view) {
  const webContents = view?.webContents;
  if (!webContents || webContents.isDestroyed()) {
    return;
  }
  if (typeof webContents.invalidate === 'function') {
    webContents.invalidate();
  }
}

function restoreMainWindowContent() {
  if (!mainWindow || mainWindow.isDestroyed()) {
    return;
  }
  if (mainWindow.isMinimized()) {
    mainWindow.restore();
  }
  if (settingsOpen && settingsView && !settingsView.webContents.isDestroyed()) {
    if (chatosView && !chatosView.webContents.isDestroyed()) {
      chatosView.setVisible(false);
    }
    settingsView.setVisible(true);
    layoutSettingsView();
    repaintView(settingsView);
    return;
  }
  if (!chatosView || chatosView.webContents.isDestroyed()) {
    createChatosView();
  } else {
    chatosView.setVisible(true);
    layoutChatosView();
    repaintView(chatosView);
  }
}

function chatosWebUrl() {
  if (developerMode) {
    return DEVELOPER_CHATOS_WEB_URL;
  }
  return (
    process.env.LOCAL_CONNECTOR_CHATOS_WEB_URL ||
    process.env.CHATOS_WEB_URL ||
    'https://app.jgoool.com'
  ).trim();
}

function localConnectorCloudBaseUrl() {
  if (developerMode) {
    return DEVELOPER_CLOUD_BASE_URL;
  }
  return (
    process.env.LOCAL_CONNECTOR_CLOUD_BASE_URL ||
    'https://local-connector.jgoool.com'
  ).trim();
}

async function refreshDeveloperModeFromCore() {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    try {
      const response = await sendIpcHttpRequest({
        endpoint: '/api/local/runtime-settings',
        method: 'GET',
        headers: localApiHeaders(false),
        body: null,
      });
      if (response.ok) {
        const settings = JSON.parse(response.body || '{}');
        const next = Boolean(settings.developer_mode);
        if (next !== developerMode) {
          developerMode = next;
          recreateChatosView();
        }
        return;
      }
    } catch {
      // Core startup is asynchronous; retry briefly before keeping the default mode.
    }
    await delay(100);
  }
}

function chatosUrlWithDesktopParam() {
  const url = new URL(chatosWebUrl());
  url.searchParams.set('desktop', 'local-connector');
  return url.toString();
}

function chatosOrigin() {
  return new URL(chatosWebUrl()).origin;
}

function isAllowedChatosUrl(url) {
  try {
    const parsed = new URL(url);
    return parsed.origin === chatosOrigin();
  } catch {
    return false;
  }
}

function handleChatosProtocolNavigation(url) {
  if (!url.startsWith('chatos-local-connector://')) {
    return false;
  }
  try {
    const parsed = new URL(url);
    if (parsed.hostname === 'auth') {
      const ticket = parsed.searchParams.get('ticket') || '';
      void authenticateDesktopTicket(ticket);
      return true;
    }
    if (parsed.hostname === 'logout') {
      void sendIpcHttpRequest({
        endpoint: '/api/local/auth/logout',
        method: 'POST',
        headers: localApiHeaders(true),
        body: '{}',
      }).catch(() => undefined);
      return true;
    }
  } catch {
    return true;
  }
  return true;
}

async function authenticateDesktopTicket(ticket) {
  const trimmed = String(ticket || '').trim();
  if (!trimmed) {
    return;
  }
  await sendIpcHttpRequest({
    endpoint: '/api/local/auth/desktop-ticket',
    method: 'POST',
    headers: localApiHeaders(true),
    body: JSON.stringify({
      cloud_base_url: localConnectorCloudBaseUrl(),
      ticket: trimmed,
    }),
  });
}

function openExternalIfSafe(url) {
  try {
    const parsed = new URL(url);
    if (parsed.protocol === 'https:' || parsed.protocol === 'http:') {
      shell.openExternal(url);
    }
  } catch {
    // Ignore invalid URLs from remote content.
  }
}

function desktopSystemPermissionStatuses() {
  if (process.platform !== 'darwin') {
    return {};
  }
  const accessibilityReady = systemPreferences.isTrustedAccessibilityClient(false);
  const screenStatus = systemPreferences.getMediaAccessStatus('screen');
  return {
    accessibility_control: {
      status: accessibilityReady ? 'ready' : 'needs_attention',
      status_label: accessibilityReady ? '已授权' : '需要授权',
      last_error: accessibilityReady ? null : 'Chat OS Local Connector 尚未获得 macOS 辅助功能权限。',
    },
    screen_recording: {
      status: screenStatus === 'granted' ? 'ready' : 'needs_attention',
      status_label: screenStatus === 'granted' ? '已授权' : '需要授权',
      last_error: screenStatus === 'granted'
        ? null
        : `macOS 屏幕录制权限状态：${screenStatus || 'unknown'}`,
    },
  };
}

function requestDesktopSystemPermission(permissionId) {
  if (process.platform === 'darwin' && permissionId === 'accessibility_control') {
    systemPreferences.isTrustedAccessibilityClient(true);
  }
  return desktopSystemPermissionStatuses();
}

function openSettingsView() {
  if (!mainWindow || mainWindow.isDestroyed()) {
    return;
  }
  settingsOpen = true;
  if (chatosView && !chatosView.webContents.isDestroyed()) {
    chatosView.setVisible(false);
  }
  if (settingsView && !settingsView.webContents.isDestroyed()) {
    settingsView.setVisible(true);
    layoutSettingsView();
    repaintView(settingsView);
    settingsView.webContents.focus();
    mainWindow.show();
    mainWindow.focus();
    return;
  }
  settingsView = new WebContentsView({
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      preload: path.join(__dirname, 'preload.cjs'),
      sandbox: true,
      backgroundThrottling: false,
    },
  });
  const settingsWebContentsId = settingsView.webContents.id;
  trustedLocalWebContents.add(settingsWebContentsId);
  mainWindow.contentView.addChildView(settingsView);
  layoutSettingsView();
  settingsView.webContents.loadFile(path.join(__dirname, '..', 'dist', 'index.html'), {
    query: { view: 'settings' },
  });
  settingsView.webContents.setWindowOpenHandler(({ url }) => {
    openExternalIfSafe(url);
    return { action: 'deny' };
  });
  settingsView.webContents.on('before-input-event', (event, input) => {
    if (input.type === 'keyDown' && input.key === 'Escape') {
      event.preventDefault();
      closeSettingsView();
    }
  });
  settingsView.webContents.once('did-finish-load', () => repaintView(settingsView));
  settingsView.webContents.once('destroyed', () => {
    if (settingsView?.webContents.id !== settingsWebContentsId) {
      return;
    }
    trustedLocalWebContents.delete(settingsWebContentsId);
    settingsView = null;
    settingsOpen = false;
    setImmediate(restoreMainWindowContent);
  });
  mainWindow.show();
  mainWindow.focus();
  settingsView.webContents.focus();
}

function closeSettingsView() {
  settingsOpen = false;
  const view = settingsView;
  settingsView = null;
  if (view) {
    trustedLocalWebContents.delete(view.webContents.id);
    if (mainWindow && !mainWindow.isDestroyed()) {
      try {
        mainWindow.contentView.removeChildView(view);
      } catch {
        // The view may already have been detached during main-window shutdown.
      }
    }
    if (!view.webContents.isDestroyed()) {
      view.webContents.close();
    }
  }
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.show();
    mainWindow.focus();
    setImmediate(restoreMainWindowContent);
  }
}

app.whenReady().then(() => {
  Menu.setApplicationMenu(null);
  ipcMain.handle('local-connector:api-request', (event, request) => {
    const rendererRequest = request && typeof request === 'object' ? request : {};
    return requestLocalApiOverIpc({ ...rendererRequest, sender: event.sender });
  });
  ipcMain.handle('local-connector:desktop-system-permissions', (event) => {
    if (!isTrustedLocalSender(event.sender)) {
      return {};
    }
    return desktopSystemPermissionStatuses();
  });
  ipcMain.handle('local-connector:desktop-system-permission-request', (event, permissionId) => {
    if (!isTrustedLocalSender(event.sender)) {
      return {};
    }
    return requestDesktopSystemPermission(String(permissionId || ''));
  });
  ipcMain.handle('local-connector:settings-open', () => {
    openSettingsView();
  });
  ipcMain.handle('local-connector:settings-close', (event) => {
    if (!settingsView || event.sender.id !== settingsView.webContents.id) {
      return false;
    }
    closeSettingsView();
    return true;
  });
  ipcMain.handle('local-connector:chatos-reload', () => {
    if (chatosView) {
      chatosView.webContents.reload();
    }
  });
  ipcMain.handle('local-connector:developer-mode', (event, enabled) => {
    if (!settingsView || event.sender.id !== settingsView.webContents.id) {
      return false;
    }
    const next = Boolean(enabled);
    if (next !== developerMode) {
      developerMode = next;
      recreateChatosView();
    }
    return true;
  });
  startCore();
  createWindow();
  void refreshDeveloperModeFromCore();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
  app.on('browser-window-focus', (_event, window) => {
    if (window === mainWindow) {
      restoreMainWindowContent();
    }
  });
});

app.on('did-become-active', () => {
  restoreMainWindowContent();
});

app.on('before-quit', () => {
  if (coreProcess) {
    coreProcess.kill();
    coreProcess = null;
  }
  if (ipcSocketDir) {
    fs.rmSync(ipcSocketDir, { recursive: true, force: true });
  }
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});
