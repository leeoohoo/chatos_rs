// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface LocalConnectorProjectRoot {
  deviceId: string;
  workspaceId: string;
  relativePath: string | null;
}

export const parseLocalConnectorProjectRoot = (
  rootPath: string | null | undefined,
): LocalConnectorProjectRoot | null => {
  const prefix = 'local://connector/';
  const value = String(rootPath || '').trim();
  if (!value.startsWith(prefix)) {
    return null;
  }
  const parts = value.slice(prefix.length).split('/').filter(Boolean);
  if (parts.length < 2) {
    return null;
  }
  try {
    return {
      deviceId: decodeURIComponent(parts[0]),
      workspaceId: decodeURIComponent(parts[1]),
      relativePath: parts.length > 2
        ? parts.slice(2).map((part) => decodeURIComponent(part)).join('/')
        : null,
    };
  } catch {
    return null;
  }
};
