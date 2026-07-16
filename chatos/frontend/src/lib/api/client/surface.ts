// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { localRuntimeBridgeAvailable } from '../localRuntime/bridge';

export const CHATOS_CLIENT_SURFACE_HEADER = 'X-Chatos-Client-Surface';
export const CHATOS_CLIENT_SURFACE_COMPAT_HEADER = 'X-Requested-With';
export const LOCAL_CONNECTOR_DESKTOP_SURFACE = 'local-connector-desktop';

export const applyClientSurfaceHeader = (headers: Headers): void => {
  if (localRuntimeBridgeAvailable()) {
    // The deployed cloud API already allows X-Requested-With in CORS preflights.
    // Send only the compatibility header until every cloud deployment accepts
    // X-Chatos-Client-Surface; sending both would still break older servers.
    headers.delete(CHATOS_CLIENT_SURFACE_HEADER);
    headers.set(CHATOS_CLIENT_SURFACE_COMPAT_HEADER, LOCAL_CONNECTOR_DESKTOP_SURFACE);
  }
};
