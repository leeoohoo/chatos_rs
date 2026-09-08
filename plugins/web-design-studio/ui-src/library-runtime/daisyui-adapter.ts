import { createDomRegistryAdapter } from './dom-registry-adapter';
import { DAISYUI_REGISTRY_BY_SLUG } from './daisyui-registry.generated';
import type { DomRegistryEntry } from './types';

export const daisyUiRuntimeAdapter = createDomRegistryAdapter({
  library: 'daisyui',
  entries: DAISYUI_REGISTRY_BY_SLUG,
  async loadEntry(slug) {
    const response = await fetch(new URL(`./daisyui/${encodeURIComponent(slug)}.json`, window.location.href));
    if (!response.ok) throw new Error(`daisyUI official example payload failed to load: ${response.status}`);
    return await response.json() as DomRegistryEntry;
  },
  className: 'daisy-runtime'
});
