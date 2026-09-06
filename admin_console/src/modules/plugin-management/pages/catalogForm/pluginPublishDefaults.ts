// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { PluginCatalogListItem, PluginVisibility } from '../../pluginTypes';

export interface ExistingCatalogPublishDefaults {
  visibility: PluginVisibility;
  featured: boolean;
  licenseId: string;
  licenseUrl: string;
  redistributable: boolean;
  publisherId: string;
}

export function findExistingCatalogPublishDefaults(
  plugins: PluginCatalogListItem[],
  marketplaceId: unknown,
  pluginName: string,
): ExistingCatalogPublishDefaults | null {
  if (typeof marketplaceId !== 'string' || !marketplaceId.trim()) return null;
  const existing = plugins.find(
    (plugin) => plugin.marketplace_id === marketplaceId && plugin.name === pluginName,
  );
  if (!existing) return null;
  return {
    visibility: existing.visibility,
    featured: existing.featured,
    licenseId: existing.license.license_id,
    licenseUrl: existing.license.license_url || '',
    redistributable: existing.license.redistributable,
    publisherId: existing.publisher.id,
  };
}
