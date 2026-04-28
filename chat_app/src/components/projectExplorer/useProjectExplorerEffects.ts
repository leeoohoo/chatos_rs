import {
  useProjectExplorerProjectLifecycle,
  useProjectExplorerSummaryPolling,
} from './useProjectExplorerProjectLifecycle';
import { useProjectExplorerUiPersistence } from './useProjectExplorerUiPersistence';

interface UseProjectExplorerEffectsParams {
  lifecycle: Parameters<typeof useProjectExplorerProjectLifecycle>[0];
  persistence: Parameters<typeof useProjectExplorerUiPersistence>[0];
  summaryPolling: Parameters<typeof useProjectExplorerSummaryPolling>[0];
}

export const useProjectExplorerEffects = ({
  lifecycle,
  persistence,
  summaryPolling,
}: UseProjectExplorerEffectsParams) => {
  useProjectExplorerProjectLifecycle(lifecycle);
  useProjectExplorerUiPersistence(persistence);
  useProjectExplorerSummaryPolling(summaryPolling);
};
