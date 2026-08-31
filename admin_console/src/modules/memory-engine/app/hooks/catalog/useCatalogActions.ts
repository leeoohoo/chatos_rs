// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { buildCatalogActions } from './actions';
import type { CatalogActionControls } from './actions/types';
import type {
  CatalogActions,
  CatalogLoaders,
  CatalogResourceCallbacks,
  MessageApi,
} from './types';

export function useCatalogActions(
  message: MessageApi,
  controls: CatalogActionControls,
  loaders: CatalogLoaders,
  callbacks?: CatalogResourceCallbacks,
): CatalogActions {
  return buildCatalogActions({ message, controls, loaders, callbacks });
}
