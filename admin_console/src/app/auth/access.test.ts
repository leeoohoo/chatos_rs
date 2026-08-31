import { describe, expect, it } from 'vitest';

import { canAccessAdminConsole, defaultAdminPath, isSuperAdmin } from './access';

describe('administration access adapter', () => {
  it('accepts both platform administrator role names', () => {
    expect(canAccessAdminConsole('super_admin')).toBe(true);
    expect(canAccessAdminConsole('admin')).toBe(true);
    expect(canAccessAdminConsole('user')).toBe(false);
  });

  it('keeps super-admin-only modules out of the normal administrator landing path', () => {
    expect(isSuperAdmin('super_admin')).toBe(true);
    expect(isSuperAdmin('admin')).toBe(false);
    expect(defaultAdminPath('super_admin')).toBe('/users/models');
    expect(defaultAdminPath('admin')).toBe('/projects/list');
  });
});
