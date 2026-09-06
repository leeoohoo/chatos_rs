// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { PluginCatalogListItem } from '../../pluginTypes';
import { findExistingCatalogPublishDefaults } from './pluginPublishDefaults';

describe('findExistingCatalogPublishDefaults', () => {
  it('inherits catalog policy fields when publishing another release', () => {
    const plugin = {
      marketplace_id: 'chatos-marketplace',
      name: 'chatos-browser-cdp',
      visibility: 'public',
      featured: true,
      license: {
        license_id: 'Apache-2.0',
        license_url: 'https://www.apache.org/licenses/LICENSE-2.0',
        redistributable: true,
      },
      publisher: { id: 'chatos' },
    } as PluginCatalogListItem;

    expect(findExistingCatalogPublishDefaults(
      [plugin],
      'chatos-marketplace',
      'chatos-browser-cdp',
    )).toEqual({
      visibility: 'public',
      featured: true,
      licenseId: 'Apache-2.0',
      licenseUrl: 'https://www.apache.org/licenses/LICENSE-2.0',
      redistributable: true,
      publisherId: 'chatos',
    });
  });

  it('does not inherit from a plugin in another marketplace', () => {
    const plugin = {
      marketplace_id: 'other-marketplace',
      name: 'chatos-browser-cdp',
    } as PluginCatalogListItem;

    expect(findExistingCatalogPublishDefaults(
      [plugin],
      'chatos-marketplace',
      'chatos-browser-cdp',
    )).toBeNull();
  });
});
