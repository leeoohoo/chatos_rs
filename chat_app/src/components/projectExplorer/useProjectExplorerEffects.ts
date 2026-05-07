import {
  useProjectExplorerProjectLifecycle,
  useProjectExplorerSummaryRealtime,
} from './useProjectExplorerProjectLifecycle';
import { useProjectExplorerUiPersistence } from './useProjectExplorerUiPersistence';

interface UseProjectExplorerEffectsParams {
  lifecycle: Parameters<typeof useProjectExplorerProjectLifecycle>[0];
  persistence: Parameters<typeof useProjectExplorerUiPersistence>[0];
  summaryRealtime: Parameters<typeof useProjectExplorerSummaryRealtime>[0];
}

export const useProjectExplorerEffects = ({
  lifecycle,
  persistence,
  summaryRealtime,
}: UseProjectExplorerEffectsParams) => {
  useProjectExplorerProjectLifecycle(lifecycle);
  useProjectExplorerUiPersistence(persistence);
  useProjectExplorerSummaryRealtime(summaryRealtime);
};
