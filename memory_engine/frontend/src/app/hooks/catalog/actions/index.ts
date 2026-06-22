import type { CatalogActions } from '../types';

import { buildCatalogModalActions } from './modal';
import { buildCatalogModelActions } from './model';
import { buildCatalogPolicyActions } from './policy';
import { buildCatalogSourceActions } from './source';
import type { CatalogActionsContext } from './types';

export function buildCatalogActions(context: CatalogActionsContext): CatalogActions {
  const modal = buildCatalogModalActions(context.controls);
  const source = buildCatalogSourceActions(context, modal.closeSourceModal);
  const model = buildCatalogModelActions(context, modal.closeModelModal);
  const policy = buildCatalogPolicyActions(context);

  return {
    ...modal,
    ...source,
    ...model,
    ...policy,
  };
}
