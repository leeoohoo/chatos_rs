// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const { app, BrowserWindow, Menu, WebContentsView, ipcMain, session, shell } = require('electron');
const { spawn } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');

let mainWindow = null;
let settingsWindow = null;
let chatosView = null;
let coreProcess = null;
const desktopAuthToken = crypto.randomBytes(32).toString('base64url');
let ipcEndpoint = null;
let ipcSocketDir = null;
const trustedLocalWebContents = new Set();
const MAX_API_ENDPOINT_LENGTH = 4096;
const MAX_API_REQUEST_BODY_BYTES = 2 * 1024 * 1024;
const MAX_API_RESPONSE_BODY_BYTES = 8 * 1024 * 1024;
const ALLOWED_API_METHODS = new Set(['GET', 'POST', 'DELETE', 'PUT', 'PATCH']);
const FORWARDED_RENDERER_HEADERS = new Set(['accept', 'content-type']);
const SHELL_HEIGHT = 52;

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
    LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN: desktopAuthToken,
    LOCAL_CONNECTOR_IPC_ENDPOINT: ipcEndpoint,
    LOCAL_CONNECTOR_OPEN_UI: '0',
    LOCAL_CONNECTOR_REQUIRE_SECURE_REMOTE: process.env.LOCAL_CONNECTOR_REQUIRE_SECURE_REMOTE || '1',
  };

  coreProcess = spawn(corePath, [], {
    cwd: path.dirname(corePath),
    env,
    stdio: 'ignore',
    windowsHide: true,
  });

  coreProcess.on('exit', () => {
    coreProcess = null;
  });
}

function createIpcEndpoint() {
  const suffix = `${process.pid}-${crypto.randomBytes(24).toString('hex')}`;
  if (process.platform === 'win32') {
    return `\\\\.\\pipe\\chatos-local-connector-${suffix}`;
  }
  const socketDir = path.join(app.getPath('temp'), `chatos-local-connector-${suffix}`);
  fs.mkdirSync(socketDir, { recursive: true, mode: 0o700 });
  return path.join(socketDir, 'core.sock');
}

function normalizeApiEndpoint(endpoint) {
  if (typeof endpoint !== 'string') {
    throw new Error('Local API endpoint must be a string');
  }
  const trimmed = endpoint.trim();
  if (!trimmed || trimmed.length > MAX_API_ENDPOINT_LENGTH || /^https?:\/\//i.test(trimmed)) {
    throw new Error('Local API endpoint is not allowed');
  }
  const url = new URL(trimmed, 'http://local-connector.internal');
  if (!url.pathname.startsWith('/api/local/')) {
    throw new Error('Local API endpoint is outside the local connector API');
  }
  return `${url.pathname}${url.search}`;
}

function normalizeApiMethod(method) {
  const normalized = typeof method === 'string' && method.trim()
    ? method.trim().toUpperCase()
    : 'GET';
  if (!ALLOWED_API_METHODS.has(normalized)) {
    throw new Error(`Local API method is not allowed: ${normalized}`);
  }
  return normalized;
}

function normalizeApiRequestBody(body) {
  if (body === undefined || body === null) {
    return null;
  }
  if (typeof body !== 'string') {
    throw new Error('Local API request body must be a string');
  }
  if (Buffer.byteLength(body, 'utf8') > MAX_API_REQUEST_BODY_BYTES) {
    throw new Error('Local API request body is too large');
  }
  return body;
}

function sanitizeRendererHeaders(inputHeaders, hasBody) {
  const headers = {
    Accept: 'application/json',
    Host: 'local-connector.ipc',
    Authorization: `Bearer ${desktopAuthToken}`,
  };

  if (inputHeaders && typeof inputHeaders === 'object') {
    for (const [name, value] of Object.entries(inputHeaders)) {
      const lowerName = name.toLowerCase();
      if (!FORWARDED_RENDERER_HEADERS.has(lowerName)) {
        continue;
      }
      if (typeof value === 'string' && value.length <= 1024) {
        headers[name] = value;
      }
    }
  }

  if (hasBody && !Object.keys(headers).some((name) => name.toLowerCase() === 'content-type')) {
    headers['Content-Type'] = 'application/json';
  }
  return headers;
}

function isTransientIpcError(error) {
  return ['ENOENT', 'ECONNREFUSED', 'ECONNRESET', 'EPIPE', 'ERROR_PIPE_BUSY'].includes(error?.code);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function requestLocalApiOverIpc(payload) {
  if (!isTrustedLocalSender(payload?.sender)) {
    throw new Error('Local API access is only available to the main connector window');
  }

  const endpoint = normalizeApiEndpoint(payload.endpoint);
  const method = normalizeApiMethod(payload.method);
  const body = normalizeApiRequestBody(payload.body);
  const headers = sanitizeRendererHeaders(payload.headers, body !== null);
  let lastError = null;

  for (let attempt = 0; attempt < 30; attempt += 1) {
    try {
      return await sendIpcHttpRequest({ endpoint, method, headers, body });
    } catch (error) {
      lastError = error;
      if (!isTransientIpcError(error)) {
        throw error;
      }
      await delay(100);
    }
  }
  throw lastError || new Error('Local connector core is not available');
}

function isTrustedLocalSender(sender) {
  return Boolean(sender && trustedLocalWebContents.has(sender.id));
}

function sendIpcHttpRequest({ endpoint, method, headers, body }) {
  return new Promise((resolve, reject) => {
    let settled = false;
    const request = http.request(
      {
        socketPath: ipcEndpoint,
        method,
        path: endpoint,
        headers,
        timeout: 15_000,
      },
      (response) => {
        const chunks = [];
        let totalBytes = 0;

        response.on('data', (chunk) => {
          totalBytes += chunk.length;
          if (totalBytes > MAX_API_RESPONSE_BODY_BYTES) {
            settled = true;
            request.destroy(new Error('Local API response body is too large'));
            reject(new Error('Local API response body is too large'));
            return;
          }
          chunks.push(chunk);
        });

        response.on('end', () => {
          if (settled) {
            return;
          }
          settled = true;
          const responseBody = Buffer.concat(chunks).toString('utf8');
          resolve({
            status: response.statusCode || 0,
            ok: Boolean(response.statusCode && response.statusCode >= 200 && response.statusCode < 300),
            headers: response.headers,
            body: responseBody,
          });
        });
      },
    );

    request.on('timeout', () => {
      request.destroy(new Error('Local API request timed out'));
    });
    request.on('error', (error) => {
      if (!settled) {
        settled = true;
        reject(error);
      }
    });

    if (body !== null) {
      request.write(body);
    }
    request.end();
  });
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
  trustedLocalWebContents.add(mainWindow.webContents.id);

  mainWindow.loadFile(path.join(__dirname, '..', 'dist', 'index.html'), {
    query: { view: 'shell' },
  });
  mainWindow.on('resize', layoutChatosView);
  mainWindow.on('closed', () => {
    trustedLocalWebContents.delete(mainWindow?.webContents?.id);
    mainWindow = null;
  });
  createChatosView();

  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url);
    return { action: 'deny' };
  });
}

function createChatosView() {
  const partition = 'persist:chatos-web';
  session.fromPartition(partition).setPermissionRequestHandler((_webContents, _permission, callback) => {
    callback(false);
  });
  chatosView = new WebContentsView({
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      partition,
    },
  });
  mainWindow.contentView.addChildView(chatosView);
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
  chatosView.webContents.loadURL(chatosUrlWithDesktopParam());
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

function chatosWebUrl() {
  return (
    process.env.LOCAL_CONNECTOR_CHATOS_WEB_URL ||
    process.env.CHATOS_WEB_URL ||
    'https://app.jgoool.com'
  ).trim();
}

function localConnectorCloudBaseUrl() {
  return (
    process.env.LOCAL_CONNECTOR_CLOUD_BASE_URL ||
    'https://local-connector.jgoool.com'
  ).trim();
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

function localApiHeaders(hasBody) {
  const headers = {
    Accept: 'application/json',
    Host: 'local-connector.ipc',
    Authorization: `Bearer ${desktopAuthToken}`,
  };
  if (hasBody) {
    headers['Content-Type'] = 'application/json';
  }
  return headers;
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

function openSettingsWindow() {
  if (settingsWindow && !settingsWindow.isDestroyed()) {
    settingsWindow.focus();
    return;
  }
  settingsWindow = new BrowserWindow({
    width: 1180,
    height: 780,
    minWidth: 920,
    minHeight: 620,
    title: 'Chat OS Local Connector Settings',
    parent: mainWindow || undefined,
    backgroundColor: '#121214',
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      preload: path.join(__dirname, 'preload.cjs'),
      sandbox: true,
    },
  });
  trustedLocalWebContents.add(settingsWindow.webContents.id);
  settingsWindow.loadFile(path.join(__dirname, '..', 'dist', 'index.html'), {
    query: { view: 'settings' },
  });
  settingsWindow.on('closed', () => {
    trustedLocalWebContents.delete(settingsWindow?.webContents?.id);
    settingsWindow = null;
  });
}

app.whenReady().then(() => {
  Menu.setApplicationMenu(null);
  ipcMain.handle('local-connector:api-request', (event, request) => {
    const rendererRequest = request && typeof request === 'object' ? request : {};
    return requestLocalApiOverIpc({ ...rendererRequest, sender: event.sender });
  });
  ipcMain.handle('local-connector:settings-open', () => {
    openSettingsWindow();
  });
  ipcMain.handle('local-connector:chatos-reload', () => {
    if (chatosView) {
      chatosView.webContents.reload();
    }
  });
  startCore();
  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
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
