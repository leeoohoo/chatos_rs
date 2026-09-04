import { INSPIRA_REGISTRY_BY_SLUG } from './inspira-registry.generated';
import { createVueRegistryAdapter } from './vue-registry-adapter';

const modules = import.meta.glob('./vendor/inspira/**/*.vue');

export const inspiraRuntimeAdapter = createVueRegistryAdapter({
  library: 'inspira',
  entries: INSPIRA_REGISTRY_BY_SLUG,
  modules
});
