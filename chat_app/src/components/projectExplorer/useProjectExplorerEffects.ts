import { useProjectExplorerProjectLifecycle } from './useProjectExplorerProjectLifecycle';
import { useProjectExplorerUiPersistence } from './useProjectExplorerUiPersistence';

interface UseProjectExplorerEffectsParams {
  lifecycle: Parameters<typeof useProjectExplorerProjectLifecycle>[0];
  persistence: Parameters<typeof useProjectExplorerUiPersistence>[0];
}

export const useProjectExplorerEffects = ({
  lifecycle,
  persistence,
}: UseProjectExplorerEffectsParams) => {
  useProjectExplorerProjectLifecycle(lifecycle);
  useProjectExplorerUiPersistence(persistence);
};
