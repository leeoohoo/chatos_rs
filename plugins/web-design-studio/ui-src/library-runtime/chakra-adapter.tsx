import type { ReactNode } from 'react';
import { ChakraProvider, defaultSystem } from '@chakra-ui/react';
import { Toaster } from 'compositions/ui/toaster';
import { CHAKRA_REGISTRY_BY_SLUG, CHAKRA_REGISTRY_MODULES } from './chakra-registry.generated';
import { createReactRegistryAdapter } from './react-registry-adapter';

function ChakraRuntimeProvider({ children }: { children: ReactNode }) {
  return (
    <ChakraProvider value={defaultSystem}>
      {children}
      <Toaster />
    </ChakraProvider>
  );
}

export const chakraRuntimeAdapter = createReactRegistryAdapter({
  library: 'chakra',
  entries: CHAKRA_REGISTRY_BY_SLUG,
  modules: CHAKRA_REGISTRY_MODULES,
  wrap: (node) => <ChakraRuntimeProvider>{node}</ChakraRuntimeProvider>
});
