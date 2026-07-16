// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const { spawn } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const MAX_UNIX_SOCKET_PATH_BYTES = 100;
const CORE_LOG_MAX_BYTES = 5 * 1024 * 1024;

function createCoreRuntime({ app, desktopAuthToken }) {
  let coreProcess = null;
  let ipcEndpoint = null;
  let ipcSocketDir = null;

  function resourcePath(...segments) {
    const packagedPath = path.join(process.resourcesPath, ...segments);
    if (fs.existsSync(packagedPath)) {
      return packagedPath;
    }
    return path.join(__dirname, 'resources', ...segments);
  }

  function bundledToolsPlatformName() {
    const os = process.platform === 'darwin'
      ? 'macos'
      : process.platform === 'win32'
        ? 'windows'
        : 'linux';
    const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
    return `${os}-${arch}`;
  }

  function bundledBrowserRuntime() {
    const toolsDir = resourcePath('bundled-tools', bundledToolsPlatformName());
    const agentBrowser = path.join(
      toolsDir,
      process.platform === 'win32' ? 'agent-browser.exe' : 'agent-browser',
    );
    const browserExecutable = process.platform === 'darwin'
      ? path.join(
          toolsDir,
          'browser',
          'Google Chrome for Testing.app',
          'Contents',
          'MacOS',
          'Google Chrome for Testing',
        )
      : process.platform === 'win32'
        ? path.join(toolsDir, 'browser', 'chrome-win64', 'chrome.exe')
        : path.join(toolsDir, 'browser', 'chrome-linux64', 'chrome');
    return {
      toolsDir,
      agentBrowser,
      browserExecutable,
    };
  }

  function coreExecutablePath() {
    const existing = String(process.env.PATH || '').split(path.delimiter).filter(Boolean);
    const candidates = [bundledBrowserRuntime().toolsDir];
    if (process.platform === 'darwin') {
      candidates.push(
        path.join(app.getPath('home'), '.docker', 'bin'),
        '/Applications/Docker.app/Contents/Resources/bin',
        '/opt/homebrew/bin',
        '/usr/local/bin',
      );
    } else if (process.platform === 'win32') {
      for (const root of [process.env.ProgramFiles, process.env['ProgramFiles(x86)']]) {
        if (root) {
          candidates.push(path.join(root, 'Docker', 'Docker', 'resources', 'bin'));
        }
      }
    } else {
      candidates.push('/usr/local/bin', '/usr/bin', '/snap/bin');
    }
    return [...new Set([...candidates, ...existing])].join(path.delimiter);
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
    const browserRuntime = bundledBrowserRuntime();
    const browserStateDir = process.platform === 'win32'
      ? path.join(app.getPath('userData'), 'browser-runtime')
      : path.join('/tmp', `chatos-agent-browser-${process.getuid?.() ?? process.pid}`);
    fs.mkdirSync(browserStateDir, { recursive: true, mode: 0o700 });
    const env = {
      ...process.env,
      PATH: coreExecutablePath(),
      CHATOS_BUNDLED_TOOLS_DIR: resourcePath('bundled-tools'),
      CHATOS_BUNDLED_SKILLS_DIR: resourcePath('skill-bundles'),
      LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN: desktopAuthToken,
      LOCAL_CONNECTOR_IPC_ENDPOINT: ipcEndpoint,
      LOCAL_CONNECTOR_OPEN_UI: '0',
      LOCAL_CONNECTOR_REQUIRE_SECURE_REMOTE: process.env.LOCAL_CONNECTOR_REQUIRE_SECURE_REMOTE || '1',
    };
    env.AGENT_BROWSER_BIN = browserRuntime.agentBrowser;
    env.AGENT_BROWSER_EXECUTABLE_PATH = browserRuntime.browserExecutable;
    env.AGENT_BROWSER_SOCKET_DIR = browserStateDir;

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
      const child = spawn(command, args, {
        stdio: 'ignore',
        windowsHide: true,
      });
      child.once('error', () => resolve());
      child.once('exit', () => resolve());
    });
  }

  async function stopCoreProcessTree() {
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

module.exports = { createCoreRuntime };
