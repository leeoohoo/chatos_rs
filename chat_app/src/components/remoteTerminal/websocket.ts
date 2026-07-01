// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export const closeWebSocketSafely = (socket: WebSocket | null | undefined) => {
  if (!socket) {
    return;
  }
  if (socket.readyState === WebSocket.OPEN) {
    socket.close();
    return;
  }
  if (socket.readyState === WebSocket.CONNECTING) {
    const closeOnOpen = () => {
      try {
        socket.close();
      } catch {
        // ignore
      }
    };
    socket.addEventListener('open', closeOnOpen, { once: true });
  }
};

export const buildWsUrl = (
  baseUrl: string,
  path: string,
  webSocketTicket?: string | null,
) => {
  const cleanedBase = baseUrl.endsWith('/') ? baseUrl.slice(0, -1) : baseUrl;
  const cleanedPath = path.startsWith('/') ? path : `/${path}`;
  const rawUrl = (cleanedBase.startsWith('http://') || cleanedBase.startsWith('https://'))
    ? cleanedBase.replace(/^http/, 'ws') + cleanedPath
    : (() => {
        const { protocol, host } = window.location;
        const wsProtocol = protocol === 'https:' ? 'wss:' : 'ws:';
        return `${wsProtocol}//${host}${cleanedBase}${cleanedPath}`;
      })();
  const wsUrl = new URL(rawUrl);
  const ticket = (webSocketTicket || '').trim();
  if (ticket) {
    wsUrl.searchParams.set('ws_ticket', ticket);
  }
  return wsUrl.toString();
};
