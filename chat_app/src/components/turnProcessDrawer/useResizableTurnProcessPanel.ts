import {
  useCallback,
  useEffect,
  useState,
  type MouseEvent as ReactMouseEvent,
} from 'react';

const MIN_PANEL_WIDTH = 360;
const DEFAULT_PANEL_WIDTH = 460;
const MAX_PANEL_WIDTH = 960;

const getMaxPanelWidth = (): number => {
  if (typeof window === 'undefined') {
    return MAX_PANEL_WIDTH;
  }
  return Math.max(MIN_PANEL_WIDTH, Math.min(MAX_PANEL_WIDTH, Math.floor(window.innerWidth * 0.75)));
};

const clampPanelWidth = (width: number, maxWidth: number = getMaxPanelWidth()): number => (
  Math.max(MIN_PANEL_WIDTH, Math.min(maxWidth, width))
);

export const useResizableTurnProcessPanel = (panelOpen: boolean) => {
  const [panelWidth, setPanelWidth] = useState<number>(DEFAULT_PANEL_WIDTH);

  useEffect(() => {
    const maxWidth = getMaxPanelWidth();
    setPanelWidth((current) => clampPanelWidth(current, maxWidth));
  }, [panelOpen]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }

    const onResize = () => {
      setPanelWidth((current) => clampPanelWidth(current));
    };

    window.addEventListener('resize', onResize);
    return () => {
      window.removeEventListener('resize', onResize);
    };
  }, []);

  useEffect(() => () => {
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  }, []);

  const handleResizeStart = useCallback((event: ReactMouseEvent<HTMLDivElement>) => {
    if (!panelOpen) {
      return;
    }

    event.preventDefault();

    const startX = event.clientX;
    const startWidth = panelWidth;
    const maxWidth = getMaxPanelWidth();

    const onMouseMove = (moveEvent: MouseEvent) => {
      const delta = startX - moveEvent.clientX;
      setPanelWidth(clampPanelWidth(startWidth + delta, maxWidth));
    };

    const stopResize = () => {
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', stopResize);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', stopResize);
  }, [panelOpen, panelWidth]);

  return {
    panelWidth,
    handleResizeStart,
  };
};
