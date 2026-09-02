import { buildApiUrl } from '@chatos/frontend-runtime';

export const ADMIN_SERVICE_BASES = {
  userService: '/api/admin/user-service',
  projectService: '/api/admin/project-service',
  taskRunner: '/api/admin/task-runner',
  pluginManagement: '/api/admin/plugin-management',
  memoryEngine: '/api/admin/memory-engine',
  configCenter: '/api/admin/config-center',
} as const;

export function stripBackendApiPrefix(path: string, prefix = '/api'): string {
  const normalized = path.startsWith('/') ? path : `/${path}`;
  if (normalized === prefix) return '/';
  if (normalized.startsWith(`${prefix}/`)) {
    return normalized.slice(prefix.length);
  }
  return normalized;
}

export function buildAdminServiceUrl(
  baseUrl: string,
  path: string,
  backendApiPrefix = '/api',
): string {
  return buildApiUrl(baseUrl, stripBackendApiPrefix(path, backendApiPrefix));
}
