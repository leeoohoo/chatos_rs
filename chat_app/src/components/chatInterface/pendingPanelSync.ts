interface PendingPanelCacheEntry<TPanel> {
  panels: TPanel[];
  stale: boolean;
}

interface SyncPendingPanelsFromCacheOrLoadOptions<TPanel> {
  cachedEntry: PendingPanelCacheEntry<TPanel> | null;
  loadPanels: () => Promise<TPanel[]>;
  applyPanels: (panels: TPanel[]) => void;
  shouldApply?: () => boolean;
  onError?: (error: unknown) => void;
}

export const syncPendingPanelsFromCacheOrLoad = <TPanel>({
  cachedEntry,
  loadPanels,
  applyPanels,
  shouldApply,
  onError,
}: SyncPendingPanelsFromCacheOrLoadOptions<TPanel>): (() => void) | undefined => {
  if (cachedEntry && !cachedEntry.stale) {
    applyPanels(cachedEntry.panels);
    return undefined;
  }

  let cancelled = false;
  void loadPanels()
    .then((panels) => {
      if (cancelled || (shouldApply && !shouldApply())) {
        return;
      }
      applyPanels(panels);
    })
    .catch((error) => {
      if (!cancelled) {
        onError?.(error);
      }
    });

  return () => {
    cancelled = true;
  };
};
