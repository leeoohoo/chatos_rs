// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { localRuntimeBridgeAvailable } from '../localRuntime/bridge';

export const CHATOS_CLIENT_SURFACE_HEADER = 'X-Chatos-Client-Surface';
export const LOCAL_CONNECTOR_DESKTOP_SURFACE = 'local-connector-desktop';

export const applyClientSurfaceHeader = (headers: Headers): void => {
  if (localRuntimeBridgeAvailable()) {
    headers.set(CHATOS_CLIENT_SURFACE_HEADER, LOCAL_CONNECTOR_DESKTOP_SURFACE);
  }
};
