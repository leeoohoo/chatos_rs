import {BridgeProtocolError, LocalConnectorBridge} from './bridge.js';
import {ExtensionController} from './controller.js';

const bridge = new LocalConnectorBridge();
const controller = new ExtensionController({bridge});
const AUTO_RECONNECT_DELAY_MS = 2000;

let pairingEnabled = false;
let reconnectTimer = null;

controller.start();
bridge.setRequestHandler((method, params) => controller.handleRequest(method, params));
bridge.onStateChange(({connected}) => {
  void chrome.action.setBadgeText({text: connected ? 'ON' : ''});
  void chrome.action.setBadgeBackgroundColor({color: '#2563EB'});
  if (connected) {
    cancelReconnect();
  } else {
    void controller.revokeAll();
    scheduleReconnect();
  }
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
  if (!pairingEnabled) return;
  await reconnectPairedBridge();
}

async function reconnectPairedBridge() {
  if (!pairingEnabled || bridge.connected) return;
  try {
    await bridge.connect({pairingRequested: false});
  } catch {
    // The native host or MCP process may still be starting. Keep the pairing and retry.
    scheduleReconnect();
  }
}

function scheduleReconnect() {
  if (!pairingEnabled || bridge.connected || reconnectTimer !== null) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    void reconnectPairedBridge();
  }, AUTO_RECONNECT_DELAY_MS);
}

function cancelReconnect() {
  if (reconnectTimer === null) return;
  clearTimeout(reconnectTimer);
  reconnectTimer = null;
}

async function handlePopupMessage(message) {
  switch (message?.action) {
    case 'status':
      return {connected: bridge.connected, paired: pairingEnabled, ...controller.status()};
    case 'connect':
      await bridge.connect({pairingRequested: true});
      pairingEnabled = true;
      await chrome.storage.local.set({paired: true});
      return {connected: true, paired: true, ...controller.status()};
    case 'share_current_tab': {
      if (!bridge.connected) {
        throw new BridgeProtocolError('extension_unavailable', 'Connect to Browser MCP first');
      }
      const [tab] = await chrome.tabs.query({active: true, currentWindow: true});
      if (!Number.isSafeInteger(tab?.id)) {
        throw new BridgeProtocolError('not_found', 'No active tab is available');
      }
      const target = await controller.shareTab(tab.id);
      return {connected: bridge.connected, paired: pairingEnabled, target, ...controller.status()};
    }
    case 'revoke_target':
      await controller.revokeTarget(message.target_id);
      return {connected: bridge.connected, paired: pairingEnabled, ...controller.status()};
    case 'disconnect':
      pairingEnabled = false;
      cancelReconnect();
      await chrome.storage.local.set({paired: false});
      bridge.notify('extension.unpair', {});
      await bridge.disconnect('user_disconnected');
      await controller.revokeAll();
      return {connected: false, paired: false, ...controller.status()};
    default:
      throw new BridgeProtocolError('invalid_request', 'Unknown popup action');
  }
}

function normalizeError(error) {
  if (error instanceof BridgeProtocolError) return error;
  return new BridgeProtocolError(
    'backend_error',
    error instanceof Error ? error.message : 'Unknown extension error'
  );
}
