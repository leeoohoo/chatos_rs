export interface InfiniteCanvasGeometry {
  width: number;
  height: number;
  contentX: number;
  contentY: number;
}

export function createInfiniteCanvasGeometry(contentWidth: number, contentHeight: number): InfiniteCanvasGeometry {
  if (![contentWidth, contentHeight].every((value) => Number.isFinite(value) && value >= 0)) throw new Error('Canvas workspace content size is invalid.');
  const width = Math.max(4800, Math.ceil(contentWidth + 2000));
  const height = Math.max(3600, Math.ceil(contentHeight + 1600));
  return {
    width,
    height,
    contentX: Math.round((width - contentWidth) / 2),
    contentY: Math.max(320, Math.round((height - contentHeight) / 2))
  };
}

export function centeredCanvasScroll(
  geometry: InfiniteCanvasGeometry,
  viewportWidth: number,
  viewportHeight: number,
  contentWidth: number,
  contentHeight: number
): { left: number; top: number } {
  return {
    left: Math.max(0, geometry.contentX - (viewportWidth - contentWidth) / 2),
    top: Math.max(0, geometry.contentY - Math.max(0, viewportHeight - contentHeight) / 2)
  };
}

export function panCanvasScroll(startLeft: number, startTop: number, pointerDeltaX: number, pointerDeltaY: number): { left: number; top: number } {
  return {
    left: Math.max(0, startLeft - pointerDeltaX),
    top: Math.max(0, startTop - pointerDeltaY)
  };
}
