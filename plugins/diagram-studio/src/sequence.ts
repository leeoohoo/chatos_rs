export const sequenceLifelineSlotCount = 40;
export const sequenceActivationSlotCount = 33;
export const legacySequenceActivationSlotCount = 9;

export type SequenceActivationSide = 'left' | 'right';
export type SequenceActivationHandleVersion = 1 | 2;

export function sequenceSlotPercentage(slot: number): number {
  const boundedSlot = Math.max(0, Math.min(sequenceLifelineSlotCount - 1, slot));
  return 12 + boundedSlot * (86 / (sequenceLifelineSlotCount - 1));
}

export function parseSequenceSlot(handleId?: string | null): number | undefined {
  const match = /^slot-(\d+)$/.exec(handleId ?? '');
  if (!match) return undefined;
  const slot = Number(match[1]);
  return Number.isSafeInteger(slot) && slot >= 0 && slot < sequenceLifelineSlotCount ? slot : undefined;
}

export function sequenceActivationSlotPercentage(slot: number, version: SequenceActivationHandleVersion = 2): number {
  const count = version === 1 ? legacySequenceActivationSlotCount : sequenceActivationSlotCount;
  const boundedSlot = Math.max(0, Math.min(count - 1, slot));
  return boundedSlot * (100 / (count - 1));
}

export function sequenceActivationHandleId(side: SequenceActivationSide, slot: number, version: SequenceActivationHandleVersion = 2): string {
  const count = version === 1 ? legacySequenceActivationSlotCount : sequenceActivationSlotCount;
  const boundedSlot = Math.max(0, Math.min(count - 1, slot));
  return version === 1 ? `activation-${side}-${boundedSlot}` : `activation-v2-${side}-${boundedSlot}`;
}

export function parseSequenceActivationHandle(handleId?: string | null): { side: SequenceActivationSide; slot: number; version: SequenceActivationHandleVersion } | undefined {
  const value = handleId ?? '';
  const current = /^activation-v2-(left|right)-(\d+)$/.exec(value);
  if (current) {
    const slot = Number(current[2]);
    if (!Number.isSafeInteger(slot) || slot < 0 || slot >= sequenceActivationSlotCount) return undefined;
    return { side: current[1] as SequenceActivationSide, slot, version: 2 };
  }
  const legacy = /^activation-(left|right)-(\d+)$/.exec(value);
  if (!legacy) return undefined;
  const slot = Number(legacy[2]);
  if (!Number.isSafeInteger(slot) || slot < 0 || slot >= legacySequenceActivationSlotCount) return undefined;
  return { side: legacy[1] as SequenceActivationSide, slot, version: 1 };
}
