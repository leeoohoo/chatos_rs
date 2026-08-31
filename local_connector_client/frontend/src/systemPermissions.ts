// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { api, type SystemPermissionItem, type SystemPermissionsResponse } from './api';

export async function loadSystemPermissions(): Promise<SystemPermissionsResponse> {
  return api.systemPermissions();
}

export function systemPermissionReady(permission: SystemPermissionItem): boolean {
  return permission.status === 'ready'
    || permission.status === 'not_applicable';
}
