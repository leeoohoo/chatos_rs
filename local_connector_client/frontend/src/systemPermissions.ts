// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { api, type SystemPermissionItem, type SystemPermissionsResponse } from './api';

type DesktopPermissionStatuses = Record<string, {
  status: string;
  status_label: string;
  last_error?: string | null;
}>;

export async function loadSystemPermissions(): Promise<SystemPermissionsResponse> {
  const response = await api.systemPermissions();
  const desktopLoader = window.chatosLocalConnector?.getDesktopSystemPermissions;
  const desktopStatuses: DesktopPermissionStatuses = desktopLoader
    ? await desktopLoader().catch(() => ({}))
    : {};
  return {
    ...response,
    items: response.items.map((item) => {
      // Computer Use runs in the bundled core/helper process, while Electron's
      // permission APIs report the shell app process. macOS TCC grants those
      // independently, so the core response is the source of truth here.
      if (item.id === 'accessibility_control' || item.id === 'screen_recording') {
        return item;
      }
      const desktop = desktopStatuses[item.id];
      return desktop ? {
        ...item,
        status: desktop.status,
        status_label: desktop.status_label,
        last_error: desktop.last_error ?? null,
      } : item;
    }),
  };
}

export function systemPermissionReady(permission: SystemPermissionItem): boolean {
  return permission.status === 'ready'
    || permission.status === 'not_applicable'
    || permission.status === 'on_demand';
}

export function permissionsForSkill(
  permissions: SystemPermissionsResponse | null,
  skillId: string,
): SystemPermissionItem[] {
  return (permissions?.items || []).filter((permission) =>
    permission.skill_ids?.includes(skillId));
}
