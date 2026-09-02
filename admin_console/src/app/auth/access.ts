import type { AdminUser } from '../../shared/api/adminApi';

export function canAccessAdminConsole(role: AdminUser['role']): boolean {
  return role === 'super_admin' || role === 'admin';
}

export function isSuperAdmin(role: AdminUser['role']): boolean {
  return role === 'super_admin';
}

export function defaultAdminPath(role: AdminUser['role']): string {
  return isSuperAdmin(role) ? '/users/models' : '/projects/list';
}
