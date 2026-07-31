// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const STABLE_IDENTIFIERS = new Map([
  ['local_connector_client_core', 'com.chatos.local-connector.core'],
  ['chatos_computer_use_helper', 'com.chatos.local-connector.computer-use-helper'],
  ['chatos_chrome_native_host', 'com.chatos.local-connector.chrome-native-host'],
  ['chatos_sandbox_mcp_server', 'com.chatos.local-connector.sandbox-mcp-server'],
]);
const APP_IDENTIFIER = 'com.chatos.local-connector';

function enabled(value) {
  return ['1', 'true', 'TRUE', 'yes', 'YES'].includes(String(value || '').trim());
}

function run(command, args) {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (result.status !== 0) {
    throw new Error([
      `${command} ${args.join(' ')} failed`,
      result.stdout,
      result.stderr,
    ].filter(Boolean).join('\n'));
  }
  return `${result.stdout || ''}${result.stderr || ''}`;
}

function findAppBundle(appOutDir) {
  const entries = fs.readdirSync(appOutDir, { withFileTypes: true });
  const app = entries.find((entry) => entry.isDirectory() && entry.name.endsWith('.app'));
  if (!app) {
    throw new Error(`Unable to find packaged .app in ${appOutDir}`);
  }
  return path.join(appOutDir, app.name);
}

function signExecutable(executablePath, identifier) {
  if (!fs.existsSync(executablePath)) {
    throw new Error(`Expected executable is missing: ${executablePath}`);
  }
  run('/usr/bin/codesign', [
    '--force',
    '--sign',
    '-',
    '--timestamp=none',
    '--identifier',
    identifier,
    executablePath,
  ]);
}

function signAppBundle(appPath) {
  const args = [
    '--force',
    '--deep',
    '--sign',
    '-',
    '--timestamp=none',
    '--identifier',
    APP_IDENTIFIER,
  ];
  const entitlementsPath = path.resolve(__dirname, '..', '..', 'entitlements.mac.plist');
  if (fs.existsSync(entitlementsPath)) {
    args.push('--entitlements', entitlementsPath);
  }
  args.push(appPath);
  run('/usr/bin/codesign', args);
}

function verifyExecutableIdentifier(executablePath, identifier) {
  const details = run('/usr/bin/codesign', ['-d', '--verbose=4', executablePath]);
  if (!details.includes(`Identifier=${identifier}`)) {
    throw new Error(`Stable local-dev code signature identifier was not applied to ${executablePath}`);
  }
}

function verifyAppBundle(appPath) {
  run('/usr/bin/codesign', ['--verify', '--deep', '--strict', appPath]);
  const details = run('/usr/bin/codesign', ['-d', '--verbose=4', appPath]);
  if (!details.includes(`Identifier=${APP_IDENTIFIER}`)) {
    throw new Error(`Stable local-dev app code signature identifier was not applied to ${appPath}`);
  }
}

module.exports = async function signMacosLocalDev(context) {
  if (process.platform !== 'darwin') {
    return;
  }
  if (enabled(process.env.CHATOS_MAC_SIGN)) {
    return;
  }
  if (!enabled(process.env.CHATOS_COMPUTER_USE_ALLOW_UNSIGNED_LOCAL_DEV)) {
    return;
  }

  const appPath = findAppBundle(context.appOutDir);
  const resourcesPath = path.join(appPath, 'Contents', 'Resources');
  for (const [name, identifier] of STABLE_IDENTIFIERS.entries()) {
    const executablePath = path.join(resourcesPath, name);
    signExecutable(executablePath, identifier);
    verifyExecutableIdentifier(executablePath, identifier);
  }
  signAppBundle(appPath);
  verifyAppBundle(appPath);
  for (const [name, identifier] of STABLE_IDENTIFIERS.entries()) {
    verifyExecutableIdentifier(path.join(resourcesPath, name), identifier);
  }
};
