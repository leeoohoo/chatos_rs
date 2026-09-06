import { createReactRegistryAdapter } from './react-registry-adapter';
import { ANTD_REGISTRY_BY_SLUG, ANTD_REGISTRY_MODULES } from './antd-registry.generated';

export const antdRuntimeAdapter = createReactRegistryAdapter({
  library: 'antd',
  entries: ANTD_REGISTRY_BY_SLUG,
  modules: ANTD_REGISTRY_MODULES,
  previewProps(_slug, props) {
    return { registryDemo: props.registryDemo };
  }
});
