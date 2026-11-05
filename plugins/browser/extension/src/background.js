import {BridgeProtocolError, LocalConnectorBridge, discoverBridges} from './bridge.js';
import {ExtensionController} from './controller.js';

const connections = new Map();
const AUTO_RECONNECT_DELAY_MS = 2000;
const ONBOARDING_PATH = 'onboarding/onboarding.html';

let pairingEnabled = false;
let reconnectTimer = null;
let synchronization = null;

chrome.runtime.onInstalled.addListener(({reason}) => {
  if (reason === 'install' || reason === 'update') void openOnboardingIfNeeded();
});

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (sender.id !== chrome.runtime.id) {
    sendResponse({ok: false, error: {code: 'permission_denied', message: 'Untrusted sender'}});
    return false;
  }
  void handlePopupMessage(message)
    .then(result => sendResponse({ok: true, result}))
    .catch(error => {
      const protocolError = normalizeError(error);
      sendResponse({
        ok: false,
        error: {code: protocolError.code, message: protocolError.message.slice(0, 1024)}
      });
    });
  return true;
});

void restorePairing();

async function restorePairing() {
  const {paired = false} = await chrome.storage.local.get('paired');
  pairingEnabled = paired;
  await updateActionBadge();
  if (pairingEnabled) {
    try {
      await synchronizeBridges();
    } catch {
      // The native host may not be available until a Browser task starts.
    }
  }
}

async function synchronizeBridges({pairingRequested = false} = {}) {
  if (!pairingEnabled && !pairingRequested) return;
  if (synchronization) return synchronization;
  synchronization = performSynchronization(pairingRequested).finally(() => {
    synchronization = null;
    scheduleReconnect();
  });
  return synchronization;
}

async function performSynchronization(pairingRequested) {
  const discovered = await discoverBridges();
  const activeIds = new Set(discovered.map(item => item.bridgeId));
  for (const bootstrap of discovered) {
    let connection = connections.get(bootstrap.bridgeId);
    if (!connection) {
      connection = createConnection(bootstrap.bridgeId);
      connections.set(bootstrap.bridgeId, connection);
    }
    if (!connection.bridge.connected) {
      try {
        await connection.bridge.connect({pairingRequested, bridgeId: bootstrap.bridgeId});
      } catch {
        // Retry this bridge on the next discovery pass without disturbing healthy tasks.
      }
    }
  }
  for (const [bridgeId, connection] of connections) {
    if (!activeIds.has(bridgeId)) {
      await removeConnection(bridgeId, connection, 'bridge_stopped');
    }
  }
  await updateActionBadge();
}

function createConnection(bridgeId) {
  const bridge = new LocalConnectorBridge();
  const controller = new ExtensionController({bridge});
  const connection = {bridgeId, bridge, controller, removed: false};
  controller.start();
  bridge.setRequestHandler((method, params) => controller.handleRequest(method, params));
  bridge.onStateChange(({connected}) => {
    void updateActionBadge();
    if (!connected && !connection.removed) {
      void controller.revokeAll();
      scheduleReconnect();
    }
  });
  return connection;
}

async function removeConnection(bridgeId, connection, reason) {
  connection.removed = true;
  connections.delete(bridgeId);
  await connection.bridge.disconnect(reason);
  await connection.controller.revokeAll();
  connection.controller.stop();
}

function scheduleReconnect() {
  if (!pairingEnabled || reconnectTimer !== null) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    void synchronizeBridges().catch(() => {
      // Discovery is best-effort while no Browser task is running.
    });
  }, AUTO_RECONNECT_DELAY_MS);
}

function cancelReconnect() {
  if (reconnectTimer === null) return;
  clearTimeout(reconnectTimer);
  reconnectTimer = null;
}

function connectedEntries() {
  return [...connections.values()].filter(connection => connection.bridge.connected);
}

function aggregateStatus() {
  const connected = connectedEntries();
  const primary = connected.at(-1);
  return {
    connected: connected.length > 0,
    paired: pairingEnabled,
    connectionCount: connected.length,
    ...(primary?.controller.status() ?? {
      targets: [], attachedTargetIds: [], sessionName: null, groupId: null, ownedTabIds: []
    })
  };
}

async function handlePopupMessage(message) {
  switch (message?.action) {
    case 'status':
      return aggregateStatus();
    case 'connect':
      pairingEnabled = true;
      cancelReconnect();
      await synchronizeBridges({pairingRequested: true});
      await chrome.storage.local.set({paired: true, onboarding_completed: true});
      await updateActionBadge();
      return aggregateStatus();
    case 'open_onboarding':
      await openOnboarding();
      return aggregateStatus();
    case 'share_current_tab': {
      const connection = connectedEntries().at(-1);
      if (!connection) {
        throw new BridgeProtocolError('extension_unavailable', 'Connect to Browser MCP first');
      }
      const [tab] = await chrome.tabs.query({active: true, currentWindow: true});
      if (!Number.isSafeInteger(tab?.id)) {
        throw new BridgeProtocolError('not_found', 'No active tab is available');
      }
      const target = await connection.controller.shareTab(tab.id);
      return {...aggregateStatus(), target};
    }
    case 'revoke_target':
      for (const connection of connectedEntries()) {
        if (connection.controller.targets().some(target => target.id === message.target_id)) {
          await connection.controller.revokeTarget(message.target_id);
          return aggregateStatus();
        }
      }
      throw new BridgeProtocolError('not_found', 'The selected target is not active');
    case 'disconnect':
      pairingEnabled = false;
      cancelReconnect();
      await chrome.storage.local.set({paired: false});
      for (const [bridgeId, connection] of [...connections]) {
        connection.bridge.notify('extension.unpair', {});
        await removeConnection(bridgeId, connection, 'user_disconnected');
      }
      await updateActionBadge();
      return aggregateStatus();
    default:
      throw new BridgeProtocolError('invalid_request', 'Unknown popup action');
  }
}

async function openOnboardingIfNeeded() {
  const {paired = false, onboarding_completed: onboardingCompleted = false} =
    await chrome.storage.local.get(['paired', 'onboarding_completed']);
  if (paired) {
    if (!onboardingCompleted) await chrome.storage.local.set({onboarding_completed: true});
    return;
  }
  if (onboardingCompleted) return;
  await openOnboarding();
}

async function openOnboarding() {
  const url = chrome.runtime.getURL(ONBOARDING_PATH);
  const tabs = await chrome.tabs.query({});
  const existing = tabs.find(tab => tab.url === url);
  if (Number.isSafeInteger(existing?.id)) {
    await chrome.tabs.update(existing.id, {active: true});
    if (Number.isSafeInteger(existing.windowId)) {
      await chrome.windows.update(existing.windowId, {focused: true});
    }
    return;
  }
  await chrome.tabs.create({url, active: true});
}

async function updateActionBadge() {
  const count = connectedEntries().length;
  const text = count > 1 ? String(Math.min(count, 99)) : count === 1 ? 'ON' : pairingEnabled ? '…' : '!';
  const color = count > 0 ? '#16A34A' : pairingEnabled ? '#D97706' : '#DC2626';
  await chrome.action.setBadgeText({text});
  await chrome.action.setBadgeBackgroundColor({color});
}

function normalizeError(error) {
  if (error instanceof BridgeProtocolError) return error;
  return new BridgeProtocolError(
    'backend_error',
    error instanceof Error ? error.message : 'Unknown extension error'
  );
}
