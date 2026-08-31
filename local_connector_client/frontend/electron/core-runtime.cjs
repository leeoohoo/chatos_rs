// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const { spawn, spawnSync } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const MAX_UNIX_SOCKET_PATH_BYTES = 100;
const CORE_LOG_MAX_BYTES = 5 * 1024 * 1024;
const CORE_RESTART_BASE_DELAY_MS = 250;
const CORE_RESTART_MAX_DELAY_MS = 5_000;
const CORE_RESTART_STABLE_WINDOW_MS = 10_000;
const USER_SHELL_PATH_TIMEOUT_MS = 10_000;
const USER_SHELL_PATH_PREFIX = '__CHATOS_PATH_BEGIN__';
const USER_SHELL_PATH_SUFFIX = '__CHATOS_PATH_END__';
const USER_SHELL_PATH_CACHE_VERSION = 1;
const USER_SHELL_PATH_CACHE_FILE = 'user-shell-path-cache.json';
const ALLOW_ADHOC_MACOS_PLUGIN_APPS_ENV = 'CHATOS_ALLOW_ADHOC_MACOS_PLUGIN_APPS';

function existingPathDirectories(value, directoryExists = defaultDirectoryExists) {
  return String(value || '')
    .split(path.delimiter)
    .map((entry) => entry.trim())
    .filter((entry) => path.isAbsolute(entry) && directoryExists(entry));
}

function defaultDirectoryExists(candidate) {
  try {
    return fs.statSync(candidate).isDirectory();
  } catch (_error) {
    return false;
  }
}

function readCachedUserShellPath(cachePath, directoryExists = defaultDirectoryExists) {
  try {
    const payload = JSON.parse(fs.readFileSync(cachePath, 'utf8'));
    if (payload?.version !== USER_SHELL_PATH_CACHE_VERSION || !Array.isArray(payload.entries)) {
      return [];
    }
    return existingPathDirectories(payload.entries.join(path.delimiter), directoryExists);
  } catch (_error) {
    return [];
  }
}

function writeCachedUserShellPath(cachePath, entries) {
  try {
    fs.mkdirSync(path.dirname(cachePath), { recursive: true });
    fs.writeFileSync(
      cachePath,
      `${JSON.stringify({ version: USER_SHELL_PATH_CACHE_VERSION, entries }, null, 2)}\n`,
      { mode: 0o600 },
    );
    fs.chmodSync(cachePath, 0o600);
    return true;
  } catch (_error) {
    return false;
  }
}

function discoverUserShellPath({
  platform = process.platform,
  env = process.env,
  spawnSyncImpl = spawnSync,
  fileExists = fs.existsSync,
  directoryExists = defaultDirectoryExists,
  onDiagnostic = () => {},
} = {}) {
  const startedAt = Date.now();
  const report = (status, details = {}) => {
    onDiagnostic({
      status,
      elapsedMs: Math.max(0, Date.now() - startedAt),
      ...details,
    });
  };
  if (platform === 'win32') {
    report('unsupported-platform');
    return [];
  }
  const configuredShell = String(env.SHELL || '').trim();
  const fallbackShell = platform === 'darwin' ? '/bin/zsh' : '/bin/sh';
  const shell = path.isAbsolute(configuredShell) && fileExists(configuredShell)
    ? configuredShell
    : fallbackShell;
  if (!fileExists(shell)) {
    report('shell-missing');
    return [];
  }
  try {
    const result = spawnSyncImpl(
      shell,
      ['-ilc', `printf "${USER_SHELL_PATH_PREFIX}%s${USER_SHELL_PATH_SUFFIX}" "$PATH"`],
      {
        env,
        encoding: 'utf8',
        maxBuffer: 1024 * 1024,
        timeout: USER_SHELL_PATH_TIMEOUT_MS,
        windowsHide: true,
        stdio: ['ignore', 'pipe', 'ignore'],
      },
    );
    if (result.error) {
      report('spawn-error', {
        errorCode: String(result.error.code || result.error.name || 'unknown'),
      });
      return [];
    }
    if (result.status !== 0) {
      report('shell-exit', {
        exitCode: Number.isInteger(result.status) ? result.status : null,
        signal: result.signal ? String(result.signal) : null,
      });
      return [];
    }
    const stdout = String(result.stdout || '');
    const prefixIndex = stdout.lastIndexOf(USER_SHELL_PATH_PREFIX);
    const suffixIndex = stdout.indexOf(
      USER_SHELL_PATH_SUFFIX,
      prefixIndex + USER_SHELL_PATH_PREFIX.length,
    );
    if (prefixIndex < 0 || suffixIndex < 0) {
      report('markers-missing');
      return [];
    }
    const discoveredPath = stdout.slice(
      prefixIndex + USER_SHELL_PATH_PREFIX.length,
      suffixIndex,
    );
    const entries = existingPathDirectories(discoveredPath, directoryExists);
    report(entries.length > 0 ? 'success' : 'no-valid-entries', {
      entryCount: entries.length,
    });
    return entries;
  } catch (error) {
    report('exception', {
      errorCode: String(error?.code || error?.name || 'unknown'),
    });
    return [];
  }
}

function coreRestartDelayMs(attempt) {
  const normalizedAttempt = Number.isInteger(attempt) && attempt > 0 ? attempt : 0;
  return Math.min(
    CORE_RESTART_BASE_DELAY_MS * (2 ** Math.min(normalizedAttempt, 5)),
    CORE_RESTART_MAX_DELAY_MS,
  );
}

function allowAdhocMacosPluginApps({
  platform = process.platform,
  env = process.env,
  executablePath = process.execPath,
  spawnSyncImpl = spawnSync,
} = {}) {
  const configured = String(env[ALLOW_ADHOC_MACOS_PLUGIN_APPS_ENV] || '').trim();
  if (configured) {
    return configured === '1';
  }
  if (platform !== 'darwin' || !path.isAbsolute(executablePath)) {
    return false;
  }
  try {
    const result = spawnSyncImpl(
      '/usr/bin/codesign',
      ['--display', '--verbose=4', executablePath],
      {
        encoding: 'utf8',
        maxBuffer: 1024 * 1024,
        timeout: 5_000,
        windowsHide: true,
        stdio: ['ignore', 'pipe', 'pipe'],
      },
    );
    if (result.error || result.status !== 0) {
      return false;
    }
    const diagnostic = `${String(result.stdout || '')}\n${String(result.stderr || '')}`;
    return diagnostic
      .split(/\r?\n/)
      .some((line) => line.trim().toLowerCase() === 'signature=adhoc');
  } catch (_error) {
    return false;
  }
}

function createCoreRuntime({
  app,
  desktopAuthToken,
  spawnImpl = spawn,
  discoverUserShellPathImpl = discoverUserShellPath,
  allowAdhocMacosPluginAppsImpl = allowAdhocMacosPluginApps,
}) {
  let coreProcess = null;
  let ipcEndpoint = null;
  let ipcSocketDir = null;
  let restartAttempt = 0;
  let restartTimer = null;
  let stableTimer = null;
  let stopRequested = false;
  let cachedCoreExecutablePath = null;
  let coreExecutablePathSource = 'fallback';
  let coreExecutablePathDiagnostic = { status: 'not-run', elapsedMs: 0 };

  function resourcePath(...segments) {
    const packagedPath = path.join(process.resourcesPath, ...segments);
    if (fs.existsSync(packagedPath)) {
      return packagedPath;
    }
    return path.join(__dirname, 'resources', ...segments);
  }

  function coreExecutablePath() {
    if (cachedCoreExecutablePath) {
      return cachedCoreExecutablePath;
    }
    const existing = existingPathDirectories(process.env.PATH);
    const shellPathCache = path.join(app.getPath('userData'), USER_SHELL_PATH_CACHE_FILE);
    const discoveredUserShellPath = discoverUserShellPathImpl({
      onDiagnostic: (diagnostic) => {
        coreExecutablePathDiagnostic = diagnostic;
      },
    });
    let userShellPath = discoveredUserShellPath;
    if (discoveredUserShellPath.length > 0) {
      coreExecutablePathSource = 'shell';
      writeCachedUserShellPath(shellPathCache, discoveredUserShellPath);
    } else {
      userShellPath = readCachedUserShellPath(shellPathCache);
      coreExecutablePathSource = userShellPath.length > 0 ? 'cache' : 'fallback';
    }
    const candidates = [resourcePath('bundled-tools')];
    if (process.platform === 'darwin') {
      candidates.push(
        '/opt/homebrew/bin',
        '/usr/local/bin',
      );
    } else if (process.platform !== 'win32') {
      candidates.push('/usr/local/bin', '/usr/bin', '/snap/bin');
    }
    cachedCoreExecutablePath = [...new Set([...candidates, ...userShellPath, ...existing])]
      .join(path.delimiter);
    return cachedCoreExecutablePath;
  }

  function startCore() {
    if (coreProcess) {
      return;
    }
    stopRequested = false;
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
      PATH: coreExecutablePath(),
      CHATOS_BUNDLED_TOOLS_DIR: resourcePath('bundled-tools'),
      LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN: desktopAuthToken,
      LOCAL_CONNECTOR_PARENT_PID: String(process.pid),
      LOCAL_CONNECTOR_IPC_ENDPOINT: ipcEndpoint,
      LOCAL_CONNECTOR_ENABLE_TCP_API: '1',
      LOCAL_CONNECTOR_OPEN_UI: '0',
      LOCAL_CONNECTOR_REQUIRE_SECURE_REMOTE: process.env.LOCAL_CONNECTOR_REQUIRE_SECURE_REMOTE || '1',
    };
    const adhocMacosPluginAppsAllowed = allowAdhocMacosPluginAppsImpl();
    if (adhocMacosPluginAppsAllowed) {
      env[ALLOW_ADHOC_MACOS_PLUGIN_APPS_ENV] = '1';
    } else {
      delete env[ALLOW_ADHOC_MACOS_PLUGIN_APPS_ENV];
    }

    const coreLog = openCoreLog();
    appendCoreLog(
      coreLog.path,
      [
        `core PATH source=${coreExecutablePathSource}`,
        `entries=${env.PATH.split(path.delimiter).length}`,
        `shell_discovery=${coreExecutablePathDiagnostic.status}`,
        `shell_elapsed_ms=${coreExecutablePathDiagnostic.elapsedMs}`,
        coreExecutablePathDiagnostic.errorCode
          ? `shell_error_code=${coreExecutablePathDiagnostic.errorCode}`
          : null,
        Number.isInteger(coreExecutablePathDiagnostic.exitCode)
          ? `shell_exit_code=${coreExecutablePathDiagnostic.exitCode}`
          : null,
        `adhoc_plugin_apps=${adhocMacosPluginAppsAllowed ? 'enabled' : 'disabled'}`,
      ].filter(Boolean).join(' '),
    );
    try {
      coreProcess = spawnImpl(corePath, [], {
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

    const startedProcess = coreProcess;
    clearStableTimer();
    stableTimer = setTimeout(() => {
      if (coreProcess === startedProcess) {
        restartAttempt = 0;
      }
      stableTimer = null;
    }, CORE_RESTART_STABLE_WINDOW_MS);
    stableTimer.unref?.();

    startedProcess.on('error', (error) => {
      appendCoreLog(coreLog.path, `core process failed to start: ${error.stack || error}`);
    });
    startedProcess.on('exit', (code, signal) => {
      appendCoreLog(coreLog.path, `core process exited: code=${code} signal=${signal}`);
      if (coreProcess === startedProcess) {
        coreProcess = null;
      }
      clearStableTimer();
      if (!stopRequested) {
        scheduleCoreRestart(coreLog.path);
      }
    });
  }

  function clearRestartTimer() {
    if (restartTimer) {
      clearTimeout(restartTimer);
      restartTimer = null;
    }
  }

  function clearStableTimer() {
    if (stableTimer) {
      clearTimeout(stableTimer);
      stableTimer = null;
    }
  }

  function scheduleCoreRestart(logPath) {
    if (stopRequested || restartTimer || coreProcess) {
      return;
    }
    const delayMs = coreRestartDelayMs(restartAttempt);
    restartAttempt += 1;
    appendCoreLog(logPath, `core process will restart in ${delayMs}ms`);
    restartTimer = setTimeout(() => {
      restartTimer = null;
      if (!stopRequested && !coreProcess) {
        startCore();
      }
    }, delayMs);
    restartTimer.unref?.();
  }

  function createIpcEndpoint() {
    const suffix = `${process.pid}-${crypto.randomBytes(24).toString('hex')}`;
    if (process.platform === 'win32') {
      return `\\.\pipe\chatos-local-connector-${suffix}`;
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

  function waitForChildExit(child, timeoutMs) {
    if (!child || child.exitCode !== null || child.signalCode !== null) {
      return Promise.resolve();
    }
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        child.removeListener('exit', onExit);
        resolve();
      }, timeoutMs);
      const onExit = () => {
        clearTimeout(timer);
        resolve();
      };
      child.once('exit', onExit);
    });
  }

  function runHiddenProcess(command, args) {
    return new Promise((resolve) => {
      const child = spawnImpl(command, args, {
        stdio: 'ignore',
        windowsHide: true,
      });
      child.once('error', () => resolve());
      child.once('exit', () => resolve());
    });
  }

  async function stopCoreProcessTree() {
    stopRequested = true;
    clearRestartTimer();
    clearStableTimer();
    const child = coreProcess;
    if (!child || !child.pid) {
      coreProcess = null;
      return;
    }
    if (process.platform === 'win32') {
      await runHiddenProcess('taskkill.exe', ['/PID', String(child.pid), '/T', '/F']);
    } else {
      child.kill();
    }
    await waitForChildExit(child, 3000);
    coreProcess = null;
  }

  function getIpcEndpoint() {
    return ipcEndpoint;
  }

  function isRunning() {
    return Boolean(coreProcess);
  }

  function cleanupIpcEndpoint() {
    stopRequested = true;
    clearRestartTimer();
    clearStableTimer();
    if (ipcSocketDir) {
      fs.rmSync(ipcSocketDir, { recursive: true, force: true });
      ipcSocketDir = null;
    }
    ipcEndpoint = null;
  }

  return {
    cleanupIpcEndpoint,
    getIpcEndpoint,
    isRunning,
    resourcePath,
    startCore,
    stopCoreProcessTree,
  };
}

module.exports = {
  allowAdhocMacosPluginApps,
  coreRestartDelayMs,
  createCoreRuntime,
  discoverUserShellPath,
  readCachedUserShellPath,
  writeCachedUserShellPath,
};
