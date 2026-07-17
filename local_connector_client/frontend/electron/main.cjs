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
const crypto = require('node:crypto');
const path = require('node:path');
const { startBundledChatosServer } = require('./bundled-chatos-server.cjs');
const { isTrustedMainFrameEvent } = require('./ipc-trust.cjs');
const { createLocalApiBridge } = require('./local-api-bridge.cjs');
const { attachRetryingViewLoader } = require('./retrying-view-loader.cjs');
const { createCoreRuntime } = require('./core-runtime.cjs');
const {
  isAllowedLocalFrontendUrl,
  isAllowedOriginUrl,
  isSafeExternalUrl,
} = require('./navigation-policy.cjs');

if (!app.requestSingleInstanceLock()) {
  app.exit(0);
}

let mainWindow = null;
let settingsView = null;
let settingsOpen = false;
let chatosView = null;
let chatosViewLoader = null;
let bundledChatosServer = null;
let shutdownStarted = false;
const desktopAuthToken = crypto.randomBytes(32).toString('base64url');
const coreRuntime = createCoreRuntime({ app, desktopAuthToken });
let developerMode = process.env.LOCAL_CONNECTOR_DEVELOPER_MODE === '1';
const trustedLocalWebContents = new Set();
const trustedRuntimeWebContents = new Set();
const localApiBridge = createLocalApiBridge({
  getIpcEndpoint: () => coreRuntime.getIpcEndpoint(),
  getDesktopAuthToken: () => desktopAuthToken,
  isTrustedSender: (sender) => isTrustedLocalSender(sender),
});
const runtimeApiBridge = createLocalApiBridge({
  getIpcEndpoint: () => coreRuntime.getIpcEndpoint(),
  getDesktopAuthToken: () => desktopAuthToken,
  isTrustedSender: (sender) => Boolean(sender && trustedRuntimeWebContents.has(sender.id)),
  endpointPrefix: '/api/local/runtime/',
});
const {
  delay,
  requestLocalApiOverIpc,
  sendIpcHttpRequest,
  localApiHeaders,
} = localApiBridge;
const SHELL_HEIGHT = 52;
const RUNTIME_SETTINGS_STARTUP_ATTEMPTS = 300;
const DEVELOPER_CHATOS_WEB_URL = (
  process.env.LOCAL_CONNECTOR_DEVELOPER_CHATOS_WEB_URL || 'http://127.0.0.1:8088'
).trim();
const DEVELOPER_CLOUD_BASE_URL = (
  process.env.LOCAL_CONNECTOR_DEVELOPER_CLOUD_BASE_URL || 'http://127.0.0.1:39230'
).trim();

async function shutdownApplication() {
  try {
    if (coreRuntime.isRunning()) {
      const response = await sendIpcHttpRequest({
        endpoint: '/api/local/sandbox/shutdown',
        method: 'POST',
        headers: localApiHeaders(true),
        body: '{}',
      });
      if (!response.ok) {
        console.warn('Local sandbox shutdown returned an error', response.status, response.body);
      }
    }
  } catch (error) {
    console.warn('Unable to request graceful Local Connector shutdown', error);
  }
  await coreRuntime.stopCoreProcessTree();
  coreRuntime.cleanupIpcEndpoint();
  if (bundledChatosServer) {
    await bundledChatosServer.close().catch(() => undefined);
    bundledChatosServer = null;
  }
  app.exit(0);
}

function isTrustedLocalSender(sender) {
  return Boolean(sender && trustedLocalWebContents.has(sender.id));
}

function isTrustedLocalEvent(event) {
  return isTrustedMainFrameEvent(
    event,
    trustedLocalWebContents,
    (url) => isAllowedLocalFrontendUrl(url, localFrontendIndexPath()),
  );
}

function isTrustedRuntimeEvent(event) {
  return isTrustedMainFrameEvent(event, trustedRuntimeWebContents, chatosOrigin());
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

  attachLocalFrontendNavigationGuard(mainWindow.webContents);
  mainWindow.loadFile(localFrontendIndexPath(), {
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
    chatosViewLoader?.dispose();
    chatosViewLoader = null;
    chatosView = null;
    trustedRuntimeWebContents.clear();
    mainWindow = null;
  });
  createChatosView();

  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    openExternalIfSafe(url);
    return { action: 'deny' };
  });
}

function createChatosView() {
  const partition = developerMode ? 'persist:chatos-web-development' : 'persist:chatos-web';
  denyWebPermissions(session.fromPartition(partition));
  chatosView = new WebContentsView({
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      preload: path.join(__dirname, 'runtime-preload.cjs'),
      sandbox: true,
      partition,
      backgroundThrottling: false,
    },
  });
  const createdView = chatosView;
  trustedRuntimeWebContents.add(chatosView.webContents.id);
  mainWindow.contentView.addChildView(chatosView);
  chatosView.setVisible(!settingsOpen);
  layoutChatosView();

  attachChatosNavigationGuard(chatosView.webContents);
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
    chatosViewLoader?.dispose();
    chatosViewLoader = null;
    chatosView = null;
    trustedRuntimeWebContents.delete(createdView.webContents.id);
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
  chatosViewLoader = attachRetryingViewLoader({
    webContents: createdView.webContents,
    load: () => createdView.webContents.loadURL(chatosUrlWithDesktopParam()),
    shouldRetry: () => (
      developerMode
      && chatosView === createdView
      && !createdView.webContents.isDestroyed()
    ),
    onLoadError: (error) => {
      if (developerMode && chatosView === createdView) {
        console.warn('Developer Chat OS page is unavailable; retrying', error);
      }
    },
  });
}

function recreateChatosView() {
  if (!mainWindow || mainWindow.isDestroyed()) {
    return;
  }
  if (chatosView && !chatosView.webContents.isDestroyed()) {
    chatosViewLoader?.dispose();
    chatosViewLoader = null;
    trustedRuntimeWebContents.delete(chatosView.webContents.id);
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
  if (!bundledChatosServer) {
    throw new Error('Bundled Chat OS frontend server is not ready');
  }
  return bundledChatosServer.origin;
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
  for (let attempt = 0; attempt < RUNTIME_SETTINGS_STARTUP_ATTEMPTS; attempt += 1) {
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
  return isAllowedOriginUrl(url, chatosOrigin());
}

function localFrontendIndexPath() {
  return path.join(__dirname, '..', 'dist', 'index.html');
}

function attachLocalFrontendNavigationGuard(webContents) {
  const guard = (event, url) => {
    if (isAllowedLocalFrontendUrl(url, localFrontendIndexPath())) {
      return;
    }
    event.preventDefault();
    openExternalIfSafe(url);
  };
  webContents.on('will-navigate', guard);
  webContents.on('will-redirect', guard);
}

function attachChatosNavigationGuard(webContents) {
  const guard = (event, url) => {
    if (handleChatosProtocolNavigation(url)) {
      event.preventDefault();
      return;
    }
    if (!isAllowedChatosUrl(url)) {
      event.preventDefault();
      openExternalIfSafe(url);
    }
  };
  webContents.on('will-navigate', guard);
  webContents.on('will-redirect', guard);
}

function denyWebPermissions(targetSession) {
  targetSession.setPermissionCheckHandler(() => false);
  targetSession.setPermissionRequestHandler((_webContents, _permission, callback) => {
    callback(false);
  });
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
  if (isSafeExternalUrl(url)) {
    shell.openExternal(url);
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
  attachLocalFrontendNavigationGuard(settingsView.webContents);
  settingsView.webContents.loadFile(localFrontendIndexPath(), {
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

app.whenReady().then(async () => {
  Menu.setApplicationMenu(null);
  denyWebPermissions(session.defaultSession);
  bundledChatosServer = await startBundledChatosServer(
    coreRuntime.resourcePath('chatos-frontend'),
  );
  ipcMain.handle('local-connector:api-request', (event, request) => {
    if (!isTrustedLocalEvent(event)) {
      throw new Error('Local Connector API access requires a trusted main frame');
    }
    const rendererRequest = request && typeof request === 'object' ? request : {};
    return requestLocalApiOverIpc({ ...rendererRequest, sender: event.sender });
  });
  ipcMain.handle('local-connector:runtime-api-request', (event, request) => {
    if (!isTrustedRuntimeEvent(event)) {
      throw new Error('Local Runtime API access requires the bundled Chat OS main frame');
    }
    const rendererRequest = request && typeof request === 'object' ? request : {};
    return runtimeApiBridge.requestLocalApiOverIpc({ ...rendererRequest, sender: event.sender });
  });
  ipcMain.handle('local-connector:desktop-system-permissions', (event) => {
    if (!isTrustedLocalEvent(event)) {
      return {};
    }
    return desktopSystemPermissionStatuses();
  });
  ipcMain.handle('local-connector:desktop-system-permission-request', (event, permissionId) => {
    if (!isTrustedLocalEvent(event)) {
      return {};
    }
    return requestDesktopSystemPermission(String(permissionId || ''));
  });
  ipcMain.handle('local-connector:settings-open', (event) => {
    if (!isTrustedLocalEvent(event)) {
      return false;
    }
    openSettingsView();
    return true;
  });
  ipcMain.handle('local-connector:settings-close', (event) => {
    if (
      !isTrustedLocalEvent(event)
      || !settingsView
      || event.sender.id !== settingsView.webContents.id
    ) {
      return false;
    }
    closeSettingsView();
    return true;
  });
  ipcMain.handle('local-connector:chatos-reload', (event) => {
    if (!isTrustedLocalEvent(event)) {
      return false;
    }
    if (chatosView) {
      chatosView.webContents.reload();
    }
    return true;
  });
  ipcMain.handle('local-connector:developer-mode', (event, enabled) => {
    if (
      !isTrustedLocalEvent(event)
      || !settingsView
      || event.sender.id !== settingsView.webContents.id
    ) {
      return false;
    }
    const next = Boolean(enabled);
    if (next !== developerMode) {
      developerMode = next;
      recreateChatosView();
    }
    return true;
  });
  coreRuntime.startCore();
  createWindow();
  void refreshDeveloperModeFromCore();

  app.on('second-instance', () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.show();
      mainWindow.focus();
      restoreMainWindowContent();
    }
  });

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
}).catch((error) => {
  console.error('Unable to start Chat OS Local Connector', error);
  app.quit();
});

app.on('did-become-active', () => {
  restoreMainWindowContent();
});

app.on('before-quit', (event) => {
  event.preventDefault();
  if (shutdownStarted) {
    return;
  }
  shutdownStarted = true;
  void shutdownApplication();
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});
