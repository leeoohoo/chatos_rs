export interface ScenePointerPosition {
  clientX: number;
  clientY: number;
}

export function scenePointerDelta(
  start: ScenePointerPosition,
  current: ScenePointerPosition,
  scale: number
): { deltaX: number; deltaY: number } {
  if (!Number.isFinite(scale) || scale <= 0) throw new Error('Scene pointer scale must be positive.');
  const round = (value: number) => Math.round(value * 10) / 10;
  return {
    deltaX: round((current.clientX - start.clientX) / scale),
    deltaY: round((current.clientY - start.clientY) / scale)
  };
}
