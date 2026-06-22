import { describe, expect, it } from 'vitest';

import { deriveParentPath } from './filesystem';

describe('domain/filesystem', () => {
  it('derives parent paths for unix and windows directories', () => {
    expect(deriveParentPath('/srv/app/src')).toBe('/srv/app');
    expect(deriveParentPath('/Users/lilei/project/my_project')).toBe('/Users/lilei/project');
    expect(deriveParentPath('/')).toBeNull();
    expect(deriveParentPath('C:\\workspace\\demo')).toBe('C:\\workspace');
    expect(deriveParentPath('C:\\')).toBe('C:\\');
  });
});
